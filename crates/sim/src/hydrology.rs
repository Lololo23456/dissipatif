//! Eau et érosion sur une carte de hauteurs : modèle des « tuyaux virtuels »
//! (Mei, Decaudin, Hu, *Fast Hydraulic Erosion Simulation and Visualization on GPU*, 2007).
//!
//! Intuition : chaque colonne du terrain est un réservoir relié à ses 4 voisines par des tuyaux.
//! La différence de niveau d'eau (sol + eau) entre deux colonnes accélère l'eau dans le tuyau
//! qui les relie, comme la pente accélère un objet. L'eau a donc de l'inertie : un flux lancé
//! continue un moment, ce qui donne de vraies vagues et de vrais courants.
//!
//! L'eau rapide arrache du sol (il passe en suspension : le sédiment), le courant le transporte,
//! l'eau lente le dépose. Une rivière creuse donc son lit dans les pentes et le comble en
//! plaine ; quand il est comblé, elle déborde et prend un autre chemin.
//!
//! Par colonne i, de voisines n ∈ {−x, +x, −z, +z}, un pas de temps fait :
//! ```text
//! 1. flux       f_in ← max(0, ρ f_in + dt · A g (h_i − h_n) / l)    h = b + d (sol + eau), ρ frottement
//!               si dt · Σ_n f_in > d_i l² : flux réduits (on ne vide pas plus que ce qu'on a)
//! 2. eau        d_i ← d_i + dt (Σ entrants − Σ sortants) / l²
//!    vitesse    V_i = débit net qui traverse la colonne / profondeur moyenne
//! 3. érosion    capacité C = K_c · sin(pente) · |V| · min(1, d / d_ref)   (ce que l'eau peut porter :
//!               plus elle est rapide, en pente et abondante, plus elle porte)
//!               si s < C : le sol cède  K_s (C − s) dt ; sinon il reçoit K_d (s − C) dt
//! 4. transport  le sédiment part par les mêmes tuyaux que l'eau, dans la même proportion
//! 4b. méandres  dans un virage, l'eau est projetée vers l'extérieur : la rive extérieure perd
//!               de la matière ∝ courbure · vitesse, déposée à l'identique côté intérieur.
//!               Le lit migre ; un virage qui s'accentue ronge plus : c'est une instabilité.
//! 5. éboulement si deux colonnes voisines diffèrent de plus que le talus, la plus haute
//!               cède de la matière à la plus basse (érosion thermique)
//! ```
//! - Intégration : Euler explicite, double tampon pour chaque champ.
//! - Bords : `Edge::Closed` (murs, l'eau reste) ou `Edge::Open` (l'eau qui atteint le bord quitte
//!   le monde, comme vers la mer : au-delà du bord, le sol continue au même niveau, sans eau).
//! - Stabilité : une onde de surface va à c = √(g d). Il faut c·dt < l (nombre de Courant < 1),
//!   sinon l'eau saute des cellules et oscille sans fin. Voir `courant_number`.

use crate::grid::Field2;

/// Directions des 4 tuyaux d'une colonne : −x, +x, −z, +z.
const DIRECTIONS: [(isize, isize); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
/// Pour la direction d, la direction qui revient : le tuyau de la voisine vers moi.
const OPPOSITE: [usize; 4] = [1, 0, 3, 2];
/// En dessous de cette profondeur, une colonne est sèche : pas de vitesse, tout se dépose.
const DRY_DEPTH: f32 = 1e-3;

/// Paramètres physiques de l'écoulement et de l'érosion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HydrologyParams {
    /// Pas de temps.
    pub dt: f32,
    /// g : accélération de la pesanteur. Plus elle est forte, plus l'eau dévale vite.
    pub gravity: f32,
    /// A / l : section d'un tuyau divisée par sa longueur. Règle la facilité d'écoulement.
    pub pipe_conductance: f32,
    /// ρ : fraction du flux gardée d'un pas à l'autre (1 = aucun frottement). Sans frottement,
    /// les lacs oscillent indéfiniment.
    pub flux_retention: f32,
    /// Pluie, en hauteur d'eau par unité de temps, sur chaque colonne.
    pub rain_rate: f32,
    /// Fraction de l'eau qui s'évapore par unité de temps.
    pub evaporation_rate: f32,
    /// K_c : quantité de sédiment que l'eau peut porter, par unité de pente et de vitesse.
    /// 0 désactive l'érosion hydraulique.
    pub sediment_capacity: f32,
    /// Pente minimale prise en compte (sin) : même en terrain plat, une eau rapide arrache un peu.
    pub min_tilt: f32,
    /// d_ref : profondeur à partir de laquelle l'eau atteint sa pleine capacité de transport.
    /// En dessous, la capacité est proportionnelle à la profondeur : une pellicule de pluie,
    /// même rapide, ne creuse presque rien.
    pub erosion_depth: f32,
    /// K_s : vitesse à laquelle le sol cède quand l'eau peut porter plus qu'elle ne porte.
    pub erosion_rate: f32,
    /// K_d : vitesse à laquelle le sédiment se dépose quand l'eau en porte trop.
    pub deposition_rate: f32,
    /// Vitesse maximale de l'eau, en cellules par unité de temps. Évite des vitesses absurdes
    /// dans une pellicule d'eau très mince (débit / profondeur quasi nulle).
    pub max_speed: f32,
    /// Talus : plus grande différence de hauteur stable entre deux colonnes voisines.
    pub talus: f32,
    /// Fraction de l'excès au-delà du talus qui s'éboule par unité de temps. 0 désactive.
    pub slump_rate: f32,
    /// Vitesse à laquelle la rive extérieure d'un virage recule (par unité de courbure et de
    /// vitesse du courant). 0 désactive la migration des méandres.
    pub bank_erosion_rate: f32,
    /// Vitesse minimale du courant pour qu'il ronge ses rives.
    pub bank_min_speed: f32,
}

impl HydrologyParams {
    /// Valeurs de départ : écoulement vif mais stable pour des profondeurs de quelques cellules,
    /// érosion lente mais visible en quelques minutes de jeu.
    pub const fn reference() -> Self {
        Self {
            dt: 0.05,
            gravity: 9.81,
            pipe_conductance: 1.0,
            flux_retention: 0.98,
            rain_rate: 0.0005,
            evaporation_rate: 0.002,
            sediment_capacity: 0.025,
            min_tilt: 0.05,
            erosion_depth: 0.5,
            erosion_rate: 0.3,
            deposition_rate: 0.3,
            max_speed: 8.0,
            talus: 1.5,
            slump_rate: 0.5,
            bank_erosion_rate: 0.02,
            bank_min_speed: 0.2,
        }
    }

    /// Les mêmes, sans érosion : le terrain ne change jamais.
    pub const fn without_erosion(self) -> Self {
        Self {
            sediment_capacity: 0.0,
            slump_rate: 0.0,
            bank_erosion_rate: 0.0,
            ..self
        }
    }
}

/// Ce qui se passe au bord du monde.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// Murs : aucun flux ne sort. L'eau totale ne change que par la pluie et l'évaporation.
    Closed,
    /// L'eau qui atteint le bord s'en va, poussée par sa propre hauteur.
    Open,
}

/// Une source : de l'eau qui jaillit d'une colonne.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub x: usize,
    pub z: usize,
    /// Volume d'eau par unité de temps.
    pub rate: f32,
}

pub struct Hydrology {
    pub params: HydrologyParams,
    pub edge: Edge,
    ground: Field2,
    ground_next: Field2,
    /// Transferts de matière d'une rive à l'autre pendant ce pas (méandres), appliqués d'un coup.
    bank_transfer: Field2,
    water: Field2,
    water_next: Field2,
    /// Sédiment en suspension, en hauteur de sol équivalente.
    sediment: Field2,
    sediment_next: Field2,
    /// Flux sortants de chaque colonne, un par direction (voir `DIRECTIONS`), toujours ≥ 0.
    flux: Vec<[f32; 4]>,
    flux_next: Vec<[f32; 4]>,
    /// Vitesse horizontale de l'eau (x, z) en cellules par unité de temps.
    velocity: Vec<[f32; 2]>,
    /// Eau ajoutée par unité de temps à chaque colonne : pluie + sources. Précalculée.
    inflow: Field2,
}

impl Hydrology {
    /// Terrain sec de hauteurs `ground`.
    pub fn new(ground: Field2, params: HydrologyParams, edge: Edge, springs: &[Spring]) -> Self {
        let (nx, nz) = (ground.nx, ground.nz);
        let zero = Field2::filled(nx, nz, 0.0);
        let mut sim = Self {
            params,
            edge,
            ground_next: ground.clone(),
            bank_transfer: zero.clone(),
            ground,
            water: zero.clone(),
            water_next: zero.clone(),
            sediment: zero.clone(),
            sediment_next: zero.clone(),
            flux: vec![[0.0; 4]; nx * nz],
            flux_next: vec![[0.0; 4]; nx * nz],
            velocity: vec![[0.0; 2]; nx * nz],
            inflow: zero,
        };
        sim.set_sources(params.rain_rate, springs);
        sim
    }

    /// Remplace la pluie et les sources.
    pub fn set_sources(&mut self, rain_rate: f32, springs: &[Spring]) {
        self.params.rain_rate = rain_rate;
        self.inflow.data.fill(rain_rate);
        for spring in springs {
            let i = self.inflow.index(spring.x, spring.z);
            // Volume par unité de temps réparti sur une colonne d'aire 1 : une hauteur.
            self.inflow.data[i] += spring.rate;
        }
    }

    /// Remplace la pluie par une carte (hauteur d'eau par unité de temps, colonne par colonne :
    /// il pleut plus sur les montagnes) et les sources.
    pub fn set_rain_map(&mut self, rain: &Field2, springs: &[Spring]) {
        assert_eq!((rain.nx, rain.nz), (self.inflow.nx, self.inflow.nz));
        self.inflow.data.copy_from_slice(&rain.data);
        for spring in springs {
            let i = self.inflow.index(spring.x, spring.z);
            self.inflow.data[i] += spring.rate;
        }
    }

    /// Hauteur du sol, par colonne.
    pub fn ground(&self) -> &Field2 {
        &self.ground
    }

    /// Hauteur d'eau au-dessus du sol, par colonne.
    pub fn water(&self) -> &Field2 {
        &self.water
    }

    /// Sédiment en suspension, par colonne.
    pub fn sediment(&self) -> &Field2 {
        &self.sediment
    }

    /// Vitesse de l'eau (x, z) dans la colonne (x, z), en cellules par unité de temps.
    pub fn velocity(&self, x: usize, z: usize) -> [f32; 2] {
        self.velocity[x + self.ground.nx * z]
    }

    /// Remplace l'eau (pour les tests et les conditions initiales). Flux et sédiment repartent
    /// de zéro.
    pub fn set_water(&mut self, water: Field2) {
        assert_eq!((water.nx, water.nz), (self.ground.nx, self.ground.nz));
        self.water = water;
        self.flux.fill([0.0; 4]);
        self.velocity.fill([0.0; 2]);
        self.sediment.data.fill(0.0);
    }

    /// Nombre de Courant c·dt/l de l'onde la plus rapide, c = √(g d_max). Doit rester < 1.
    pub fn courant_number(&self) -> f32 {
        let max_depth = self.water.data.iter().copied().fold(0.0, f32::max);
        (self.params.gravity * max_depth).sqrt() * self.params.dt
    }

    /// Avance d'un pas de temps dt. Aucune allocation.
    pub fn step(&mut self) {
        self.step_water();
        if self.params.sediment_capacity > 0.0 {
            self.step_erosion();
            self.step_transport();
            if self.params.bank_erosion_rate > 0.0 {
                self.step_meander();
            }
        } else {
            // Pas d'érosion : le sol « après érosion » est le sol courant.
            self.ground_next.data.copy_from_slice(&self.ground.data);
            self.sediment.data.fill(0.0);
        }
        // L'éboulement lit `ground_next` (le sol après érosion) et écrit dans `ground`.
        if self.params.slump_rate > 0.0 {
            self.step_slump();
        } else {
            std::mem::swap(&mut self.ground, &mut self.ground_next);
        }
    }

    /// Étapes 1 et 2 : flux, eau, vitesse. Écrit `water` (après échange) et `velocity`.
    fn step_water(&mut self) {
        let HydrologyParams {
            dt,
            gravity,
            pipe_conductance,
            flux_retention,
            evaporation_rate,
            max_speed,
            ..
        } = self.params;
        let (nx, nz) = (self.ground.nx, self.ground.nz);
        let (ground, water, inflow) = (&self.ground.data, &self.water.data, &self.inflow.data);
        // Eau d'une colonne après la pluie et les sources de ce pas (calculée à la volée, sans
        // écrire dans `water` : on ne modifie jamais l'état courant pendant un pas).
        let wet = |i: usize| water[i] + dt * inflow[i];
        let neighbour = |x: usize, z: usize, d: usize| neighbour_index(nx, nz, x, z, d);

        // 1. Flux : la différence de niveau accélère l'eau dans chaque tuyau.
        for z in 0..nz {
            for x in 0..nx {
                let i = x + nx * z;
                let depth = wet(i);
                let level = ground[i] + depth;
                let mut out = [0.0; 4];
                for (d, flux) in out.iter_mut().enumerate() {
                    let neighbour_level = match (neighbour(x, z, d), self.edge) {
                        (Some(n), _) => ground[n] + wet(n),
                        // Bord ouvert : le sol continue au même niveau, sans eau.
                        (None, Edge::Open) => ground[i],
                        (None, Edge::Closed) => continue,
                    };
                    let gain = dt * pipe_conductance * gravity * (level - neighbour_level);
                    *flux = (flux_retention * self.flux[i][d] + gain).max(0.0);
                }
                // On ne peut pas vider plus d'eau que la colonne n'en contient pendant dt.
                let total = out.iter().sum::<f32>() * dt;
                if total > depth {
                    let scale = if total > 0.0 { depth / total } else { 0.0 };
                    out.iter_mut().for_each(|f| *f *= scale);
                }
                self.flux_next[i] = out;
            }
        }

        // 2. Eau : ce qui entre par les tuyaux des voisines moins ce qui sort par les miens.
        let flux = &self.flux_next;
        let water_next = &mut self.water_next.data;
        for z in 0..nz {
            for x in 0..nx {
                let i = x + nx * z;
                let incoming = |d: usize| neighbour(x, z, d).map_or(0.0, |n| flux[n][OPPOSITE[d]]);
                let outgoing: f32 = flux[i].iter().sum();
                let total_in: f32 = (0..4).map(incoming).sum();
                let before = wet(i);
                let depth = before + dt * (total_in - outgoing);
                // max(0) : ne retire que les arrondis flottants, la limitation garantit déjà ≥ 0.
                water_next[i] = (depth * (1.0 - evaporation_rate * dt)).max(0.0);

                // Vitesse : débit net qui traverse la colonne dans chaque axe (moyenne de ce
                // qui passe à gauche et à droite), divisé par la profondeur moyenne du pas.
                let mean_depth = 0.5 * (before + depth);
                self.velocity[i] = if mean_depth > DRY_DEPTH {
                    let through_x = 0.5 * (incoming(0) - flux[i][0] + flux[i][1] - incoming(1));
                    let through_z = 0.5 * (incoming(2) - flux[i][2] + flux[i][3] - incoming(3));
                    let v = [through_x / mean_depth, through_z / mean_depth];
                    let speed = (v[0] * v[0] + v[1] * v[1]).sqrt();
                    let scale = if speed > max_speed {
                        max_speed / speed
                    } else {
                        1.0
                    };
                    [v[0] * scale, v[1] * scale]
                } else {
                    [0.0; 2]
                };
            }
        }

        std::mem::swap(&mut self.water, &mut self.water_next);
        std::mem::swap(&mut self.flux, &mut self.flux_next);
    }

    /// Étape 3 : l'eau arrache ou dépose. Lit `ground`, `sediment`, écrit `ground_next` et
    /// `sediment_next`.
    fn step_erosion(&mut self) {
        let HydrologyParams {
            dt,
            sediment_capacity,
            min_tilt,
            erosion_rate,
            deposition_rate,
            erosion_depth,
            ..
        } = self.params;
        let (nx, nz) = (self.ground.nx, self.ground.nz);
        let ground = &self.ground;
        for z in 0..nz {
            for x in 0..nx {
                let i = x + nx * z;
                let [vx, vz] = self.velocity[i];
                let speed = (vx * vx + vz * vz).sqrt();
                // Pente locale par différences centrées (décentrées au bord).
                let slope_x = (ground.get((x + 1).min(nx - 1), z)
                    - ground.get(x.saturating_sub(1), z))
                    / ((x + 1).min(nx - 1) - x.saturating_sub(1)) as f32;
                let slope_z = (ground.get(x, (z + 1).min(nz - 1))
                    - ground.get(x, z.saturating_sub(1)))
                    / ((z + 1).min(nz - 1) - z.saturating_sub(1)) as f32;
                let gradient = (slope_x * slope_x + slope_z * slope_z).sqrt();
                let sin_tilt = (gradient / (1.0 + gradient * gradient).sqrt()).max(min_tilt);

                let carried = self.sediment.data[i];
                let depth = self.water.data[i];
                let capacity = if depth > DRY_DEPTH {
                    sediment_capacity * sin_tilt * speed * (depth / erosion_depth).min(1.0)
                } else {
                    0.0 // colonne sèche : tout se dépose
                };
                // Positif : le sol cède. Négatif : le sédiment se dépose.
                let exchange = if capacity > carried {
                    // On n'arrache pas plus de sol qu'il n'en reste : jamais sous zéro.
                    (erosion_rate * (capacity - carried) * dt).min(ground.data[i].max(0.0))
                } else {
                    -(deposition_rate * (carried - capacity) * dt).min(carried)
                };
                self.ground_next.data[i] = ground.data[i] - exchange;
                self.sediment_next.data[i] = carried + exchange;
            }
        }
    }

    /// Étape 4 : le sédiment voyage avec l'eau, par les mêmes tuyaux. La fraction de l'eau
    /// d'une colonne qui part par un tuyau pendant ce pas emporte la même fraction de son
    /// sédiment. Ce qu'une colonne perd, sa voisine le reçoit exactement : la matière est
    /// conservée (sauf ce qui sort par un bord ouvert). Lit `sediment_next`, écrit `sediment`.
    fn step_transport(&mut self) {
        let dt = self.params.dt;
        let (nx, nz) = (self.ground.nx, self.ground.nz);
        let source = &self.sediment_next.data;
        let flux = &self.flux;
        // Eau de chaque colonne au début du pas (après pluie et sources) : après l'échange des
        // tampons de `step_water`, `water_next` contient encore l'eau du début du pas.
        let (old_water, inflow) = (&self.water_next.data, &self.inflow.data);
        // Fraction de l'eau de la colonne i qui part par le tuyau d (≤ 1 grâce à la limitation).
        let leaving = |i: usize, d: usize| {
            let depth = old_water[i] + dt * inflow[i];
            if depth > DRY_DEPTH {
                (flux[i][d] * dt / depth).min(1.0)
            } else {
                0.0
            }
        };
        for z in 0..nz {
            for x in 0..nx {
                let i = x + nx * z;
                let kept = 1.0 - (0..4).map(|d| leaving(i, d)).sum::<f32>();
                // Ce que chaque voisine envoie vers moi : son tuyau dans la direction opposée.
                let mut received = 0.0;
                for (d, &back) in OPPOSITE.iter().enumerate() {
                    if let Some(n) = neighbour_index(nx, nz, x, z, d) {
                        received += source[n] * leaving(n, back);
                    }
                }
                self.sediment.data[i] = source[i] * kept.max(0.0) + received;
            }
        }
    }

    /// Étape 4b : migration des méandres. Lit `ground_next`, la vitesse et l'eau ; écrit les
    /// transferts dans `bank_transfer`, puis les applique à `ground_next`.
    fn step_meander(&mut self) {
        let HydrologyParams {
            dt,
            bank_erosion_rate,
            bank_min_speed,
            erosion_depth,
            ..
        } = self.params;
        let (nx, nz) = (self.ground.nx, self.ground.nz);
        let ground = &self.ground_next.data;
        let water = &self.water.data;
        let velocity = &self.velocity;
        // Direction du courant (vecteur unitaire) dans une colonne, si l'eau y coule assez vite.
        let direction = |i: usize| {
            let [vx, vz] = velocity[i];
            let speed = (vx * vx + vz * vz).sqrt();
            (speed > bank_min_speed).then(|| [vx / speed, vz / speed])
        };
        // Colonne voisine (parmi les 8) la plus proche dans la direction `d`.
        let step_towards = |x: usize, z: usize, d: [f32; 2]| -> Option<usize> {
            let x = x.checked_add_signed(d[0].round() as isize)?;
            let z = z.checked_add_signed(d[1].round() as isize)?;
            (x < nx && z < nz).then(|| x + nx * z)
        };

        self.bank_transfer.data.fill(0.0);
        for z in 0..nz {
            for x in 0..nx {
                let i = x + nx * z;
                let Some(d) = direction(i) else { continue };
                // Courbure : de combien la direction tourne entre la colonne amont et l'aval.
                let (Some(up), Some(down)) =
                    (step_towards(x, z, [-d[0], -d[1]]), step_towards(x, z, d))
                else {
                    continue;
                };
                let (Some(d_up), Some(d_down)) = (direction(up), direction(down)) else {
                    continue;
                };
                let Some(outer) = outer_bank_offset(d_up, d_down, d) else {
                    continue;
                };
                let (Some(bank), Some(inner)) = (
                    step_towards(x, z, outer),
                    step_towards(x, z, [-outer[0], -outer[1]]),
                ) else {
                    continue;
                };
                // Une rive, c'est du sol plus haut que mon lit : on ne la ronge que jusqu'à lui.
                let height_above_bed = ground[bank] - ground[i];
                if height_above_bed <= 0.0 {
                    continue;
                }
                let [vx, vz] = velocity[i];
                let speed = (vx * vx + vz * vz).sqrt();
                let curvature = (d_up[0] * d_down[1] - d_up[1] * d_down[0]).abs();
                let depth_factor = (water[i] / erosion_depth).min(1.0);
                let amount = (bank_erosion_rate * curvature * speed * depth_factor * dt)
                    .min(0.5 * height_above_bed);
                // Dépôt côté intérieur, dans l'eau (barre de méandre), sinon dans mon propre lit.
                let deposit = if water[inner] > DRY_DEPTH { inner } else { i };
                self.bank_transfer.data[bank] -= amount;
                self.bank_transfer.data[deposit] += amount;
            }
        }
        for (b, t) in self
            .ground_next
            .data
            .iter_mut()
            .zip(&self.bank_transfer.data)
        {
            *b += t;
        }
    }

    /// Étape 5 : éboulement. Entre deux colonnes voisines, la part de la différence de hauteur
    /// qui dépasse le talus glisse de la plus haute vers la plus basse. L'échange est
    /// antisymétrique (ce que l'une perd, l'autre le gagne) : la matière est conservée.
    /// Lit `ground_next`, écrit `ground`.
    fn step_slump(&mut self) {
        let HydrologyParams {
            dt,
            talus,
            slump_rate,
            ..
        } = self.params;
        let (nx, nz) = (self.ground.nx, self.ground.nz);
        let source = &self.ground_next.data;
        for z in 0..nz {
            for x in 0..nx {
                let i = x + nx * z;
                let mut change = 0.0;
                for d in 0..4 {
                    if let Some(n) = neighbour_index(nx, nz, x, z, d) {
                        let difference = source[n] - source[i];
                        let excess = (difference.abs() - talus).max(0.0);
                        change += difference.signum() * excess;
                    }
                }
                // Le facteur 1/8 garde le schéma stable : jamais plus d'un quart de l'excès
                // n'est échangé par voisine et par pas, même avec slump_rate · dt = 1.
                self.ground.data[i] = source[i] + slump_rate * dt * change / 8.0;
            }
        }
    }
}

/// Côté de la rive extérieure d'un virage, perpendiculaire au courant `d`, sachant la direction
/// en amont `d_up` et en aval `d_down` (vecteurs unitaires (x, z)). `None` en ligne droite.
///
/// Le courant tourne vers la gauche (de x vers z) si d_up × d_down > 0 : l'extérieur est alors
/// à droite. La gauche de d est d tourné de +90° : (−d_z, d_x).
fn outer_bank_offset(d_up: [f32; 2], d_down: [f32; 2], d: [f32; 2]) -> Option<[f32; 2]> {
    let turn = d_up[0] * d_down[1] - d_up[1] * d_down[0];
    if turn.abs() < 1e-3 {
        return None;
    }
    let left = [-d[1], d[0]];
    let side = -turn.signum();
    Some([side * left[0], side * left[1]])
}

/// Indice de la voisine de (x, z) dans la direction d, ou `None` au bord.
#[inline]
fn neighbour_index(nx: usize, nz: usize, x: usize, z: usize, d: usize) -> Option<usize> {
    let (dx, dz) = DIRECTIONS[d];
    let (x, z) = (x.checked_add_signed(dx)?, z.checked_add_signed(dz)?);
    (x < nx && z < nz).then(|| x + nx * z)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ni pluie, ni évaporation, ni érosion : seule la dynamique de l'écoulement.
    fn dry_params() -> HydrologyParams {
        HydrologyParams {
            rain_rate: 0.0,
            evaporation_rate: 0.0,
            ..HydrologyParams::reference().without_erosion()
        }
    }

    /// Ni pluie ni évaporation, mais érosion.
    fn eroding_params() -> HydrologyParams {
        HydrologyParams {
            rain_rate: 0.0,
            evaporation_rate: 0.0,
            ..HydrologyParams::reference()
        }
    }

    /// Une cuvette : le sol monte vers les bords.
    fn bowl(n: usize) -> Field2 {
        let mut ground = Field2::filled(n, n, 0.0);
        let c = (n - 1) as f32 / 2.0;
        for z in 0..n {
            for x in 0..n {
                let r2 = (x as f32 - c).powi(2) + (z as f32 - c).powi(2);
                ground.set(x, z, 0.02 * r2);
            }
        }
        ground
    }

    /// Un plan incliné qui descend vers z croissant.
    fn slope(nx: usize, nz: usize, drop_per_cell: f32) -> Field2 {
        let mut ground = Field2::filled(nx, nz, 0.0);
        for z in 0..nz {
            for x in 0..nx {
                ground.set(x, z, drop_per_cell * (nz - z) as f32);
            }
        }
        ground
    }

    #[test]
    fn still_lake_stays_still() {
        let mut sim = Hydrology::new(Field2::filled(8, 8, 3.0), dry_params(), Edge::Closed, &[]);
        sim.set_water(Field2::filled(8, 8, 1.0));
        for _ in 0..100 {
            sim.step();
        }
        assert!(sim.water().data.iter().all(|&d| d == 1.0));
    }

    #[test]
    fn closed_world_conserves_water() {
        let mut sim = Hydrology::new(bowl(16), dry_params(), Edge::Closed, &[]);
        let mut water = Field2::filled(16, 16, 0.0);
        for z in 0..4 {
            for x in 0..4 {
                water.set(x + 2, z + 2, 2.0);
            }
        }
        sim.set_water(water);
        let before = sim.water().sum();
        for _ in 0..2000 {
            sim.step();
            assert!(sim.water().data.iter().all(|&d| d >= 0.0 && d.is_finite()));
        }
        let after = sim.water().sum();
        assert!((after - before).abs() < 1e-4 * before, "{before} → {after}");
    }

    #[test]
    fn water_settles_flat_in_a_bowl() {
        let mut sim = Hydrology::new(bowl(16), dry_params(), Edge::Closed, &[]);
        let mut water = Field2::filled(16, 16, 0.0);
        water.set(3, 3, 30.0);
        sim.set_water(water);
        for _ in 0..6000 {
            sim.step();
        }
        // Le niveau (sol + eau) des colonnes mouillées est le même partout : un lac plat.
        let levels: Vec<f32> = (0..16 * 16)
            .filter(|&i| sim.water().data[i] > 0.05)
            .map(|i| sim.ground().data[i] + sim.water().data[i])
            .collect();
        assert!(levels.len() > 10, "le lac a disparu");
        let (lo, hi) = levels
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), &l| (lo.min(l), hi.max(l)));
        assert!(hi - lo < 0.02, "surface pas plate : {lo} … {hi}");
        // Un lac immobile n'a pas de courant.
        let fastest = (0..16)
            .flat_map(|z| (0..16).map(move |x| (x, z)))
            .map(|(x, z)| {
                let [vx, vz] = sim.velocity(x, z);
                (vx * vx + vz * vz).sqrt()
            })
            .fold(0.0, f32::max);
        assert!(fastest < 0.05, "courant résiduel {fastest}");
    }

    #[test]
    fn open_edges_drain_a_slope() {
        let mut sim = Hydrology::new(slope(16, 16, 0.2), dry_params(), Edge::Open, &[]);
        sim.set_water(Field2::filled(16, 16, 0.5));
        let before = sim.water().sum();
        for _ in 0..4000 {
            sim.step();
        }
        assert!(sim.water().sum() < 0.05 * before, "l'eau n'est pas partie");
    }

    #[test]
    fn spring_flows_downhill() {
        let spring = Spring {
            x: 8,
            z: 2,
            rate: 0.5,
        };
        let mut sim = Hydrology::new(slope(16, 24, 0.3), dry_params(), Edge::Open, &[spring]);
        for _ in 0..2000 {
            sim.step();
        }
        // L'eau est arrivée en bas de la pente, et pas en haut derrière la source.
        assert!(sim.water().get(8, 20) > 0.001);
        assert!(sim.water().get(8, 0) < sim.water().get(8, 20));
        // Et elle coule vers z croissant.
        assert!(sim.velocity(8, 12)[1] > 0.0);
    }

    #[test]
    fn still_water_does_not_erode() {
        let mut sim = Hydrology::new(
            Field2::filled(8, 8, 3.0),
            eroding_params(),
            Edge::Closed,
            &[],
        );
        sim.set_water(Field2::filled(8, 8, 1.0));
        for _ in 0..200 {
            sim.step();
        }
        assert!(sim.ground().data.iter().all(|&b| b == 3.0));
        assert!(sim.sediment().data.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn a_stream_digs_its_bed_and_conserves_matter() {
        let spring = Spring {
            x: 8,
            z: 1,
            rate: 1.0,
        };
        let initial = slope(16, 32, 0.25);
        let mut sim = Hydrology::new(initial.clone(), eroding_params(), Edge::Closed, &[spring]);
        let matter = |sim: &Hydrology| sim.ground().sum() + sim.sediment().sum();
        let before = matter(&sim);
        for _ in 0..3000 {
            sim.step();
        }
        assert!(sim.ground().data.iter().all(|b| b.is_finite()));
        // Le lit sous le courant s'est creusé.
        let dug = initial.get(8, 10) - sim.ground().get(8, 10);
        assert!(dug > 0.01, "lit creusé de {dug}");
        // Sol + sédiment conservé (monde fermé), aux arrondis flottants près.
        let after = matter(&sim);
        assert!((after - before).abs() < 1e-4 * before, "{before} → {after}");
    }

    #[test]
    fn a_spike_slumps_down_to_the_talus() {
        let mut ground = Field2::filled(9, 9, 0.0);
        ground.set(4, 4, 10.0);
        let params = HydrologyParams {
            slump_rate: 5.0,
            ..eroding_params()
        };
        let mut sim = Hydrology::new(ground, params, Edge::Closed, &[]);
        let before = sim.ground().sum();
        for _ in 0..4000 {
            sim.step();
        }
        let after = sim.ground().sum();
        assert!((after - before).abs() < 1e-3, "{before} → {after}");
        let steepest = (0..9)
            .flat_map(|z| (0..8).map(move |x| (x, z)))
            .map(|(x, z)| (sim.ground().get(x, z) - sim.ground().get(x + 1, z)).abs())
            .fold(0.0, f32::max);
        assert!(steepest < params.talus + 0.05, "pente restante {steepest}");
    }

    #[test]
    fn outer_bank_is_on_the_outside_of_the_turn() {
        // On remonte vers +z puis on tourne vers +x (virage à droite vu d'en haut, x vers la
        // droite, z vers le haut) : le centre du virage est au sud-est, l'extérieur au
        // nord-ouest (−x, +z).
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let outer = outer_bank_offset([0.0, 1.0], [1.0, 0.0], [s, s]).unwrap();
        assert!(outer[0] < 0.0 && outer[1] > 0.0, "{outer:?}");
        // Le virage inverse met l'extérieur de l'autre côté.
        let outer = outer_bank_offset([1.0, 0.0], [0.0, 1.0], [s, s]).unwrap();
        assert!(outer[0] > 0.0 && outer[1] < 0.0, "{outer:?}");
        // En ligne droite, pas de rive extérieure.
        assert!(outer_bank_offset([0.0, 1.0], [0.0, 1.0], [0.0, 1.0]).is_none());
    }

    #[test]
    fn rain_map_adds_exactly_its_water() {
        // Monde fermé, sans évaporation : l'eau totale croît du total de la carte de pluie.
        let mut rain = Field2::filled(8, 8, 0.0);
        rain.set(2, 3, 1.0);
        rain.set(5, 5, 0.5);
        let mut sim = Hydrology::new(bowl(8), dry_params(), Edge::Closed, &[]);
        sim.set_rain_map(&rain, &[]);
        let steps = 100;
        for _ in 0..steps {
            sim.step();
        }
        let expected = 1.5 * dry_params().dt as f64 * steps as f64;
        assert!(
            (sim.water().sum() - expected).abs() < 1e-3,
            "{}",
            sim.water().sum()
        );
    }

    #[test]
    fn same_inputs_are_bitwise_identical() {
        let run = || {
            let spring = Spring {
                x: 4,
                z: 4,
                rate: 1.0,
            };
            let mut sim = Hydrology::new(
                bowl(12),
                HydrologyParams::reference(),
                Edge::Open,
                &[spring],
            );
            for _ in 0..500 {
                sim.step();
            }
            (sim.water().data.clone(), sim.ground().data.clone())
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn courant_number_stays_below_one_in_a_deep_lake() {
        let mut sim = Hydrology::new(bowl(16), dry_params(), Edge::Closed, &[]);
        sim.set_water(Field2::filled(16, 16, 4.0));
        assert!(sim.courant_number() < 1.0, "{}", sim.courant_number());
    }
}
