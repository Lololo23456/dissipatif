# Phase 2 : architecture et découpage du travail

Contrat technique pour coder la tranche verticale décrite dans `phase-2.md`. Chaque lot (A1, A2, …) est confié à un agent qui travaille dans sa propre copie du dépôt ; ce document fixe ce qui est partagé, pour que les lots se fusionnent sans se contredire. Les cartes détaillées du code existant (fichiers, lignes, pièges) ont été produites avant le travail ; les points essentiels sont repris ici.

## 1. Règles communes à tous les lots

- **Copie de travail** : `git worktree add /home/user/wt/<lot> -b p2/<lot> phase-2`, travailler et committer là (messages en français, comme l'historique). Ne jamais toucher `/home/user/repo` (la copie principale) ni la branche `phase-2`.
- **Conventions du projet** (`CLAUDE.md`, `.claude/rules/*.md`) : code, identifiants et commentaires en anglais ; textes du jeu, docs et `eprintln!` en français ; pas de `unwrap()` dans le code de jeu hors initialisation ; pas d'allocation dans les boucles de pas (tampons de travail réutilisés, comme `plant_changes`) ; double tampon pour toute grille ; aléa seulement par `SplitMix64` à graine explicite et sauvegardée, ou par hachage `world::noise::hash_unit(seed ^ K, &[..])` ; couleurs dans `crates/render/src/palette.rs`, lisibles par la luminance et pas seulement la teinte ; alignement Rust ↔ WGSL documenté et testé (`size_of`).
- **`crates/sim`** : `Oregonator::step` est l'exercice de l'utilisateur. **Ne pas l'écrire**, ni aucun intégrateur numérique dans `crates/sim`. On peut y ajouter des tests, des accesseurs sans contenu numérique, et de l'outillage dans `crates/lab`.
- **Le jeu ne modifie le monde que par des événements** : `GameState::apply` et `step` prennent `&World` ; une modification du monde est un `Event` appliqué par `App::on_event` (et à la main dans les tests).
- **Le tore** : toute distance ou direction passe par `world.nearest(from, to)` ; toute position stockée par `world.wrap`. `Soil::patch` ne ramène pas les coordonnées : lui passer une position déjà ramenée.
- **Temps** : ce qui se compte en jours de jeu utilise `STEP * time_scale` ou `now.days` ; la physique des objets (feu, cuisson) reste en temps réel (`STEP`). Les nouveaux animaux en `GameState` suivent le modèle des écureuils (`time_scale.round()` sous-pas) ou intègrent `dt * time_scale` si leur modèle le permet (préférable pour le coût).
- **Sauvegarde** : la version passe à **7 pour toute la phase 2** (déjà fait dans le socle). **Ne pas changer `save::VERSION`.** Ajouter l'état nouveau en fin de `GameState::save` / `load` dans l'ordre du registre (§3.6) et l'inclure dans le test `a_saved_game_goes_on_exactly_as_before`.
- **Énumérations** : on n'ajoute qu'en fin de liste, avec les numéros réservés au §3.
- **Vérifier avant de rendre** : `cargo test --workspace` (sauf les tests ignorés), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all`, `naga` sur les shaders modifiés, et au moins une capture (`cargo run --release -p game -- --capture …`) lue avec l'outil Read pour chaque chose visible. Le rendu sans écran marche ici (lavapipe), ~6 s par image.
- **Documenter** : chaque lot ajoute une entrée datée à `docs/decisions.md` (ce qui a été décidé, pourquoi), met à jour les drapeaux et touches dans `CLAUDE.md` (section Commandes) et le doc de tête de `main.rs`, et les fiches de `docs/reactions/` quand un modèle change.
- **Valeurs par défaut** : quand `phase-2.md` laisse une question « à trancher », prendre la proposition marquée « proposé » et la noter dans `decisions.md` comme provisoire.

## 2. Le socle (fait)

- **Matériaux** : la table passe à 64 entrées (`MATERIAL_SLOTS`, `voxel.wgsl`). Nouveaux identifiants, à utiliser tels quels : `Mud 47`, `Reed 48` et `ReedHead 49` (plantes : `is_plant`), `Timber 50`, `Wattle 51`, `Daub 52`, `Thatch 53`, `Plank 54` (construits : `Material::is_built`, jamais creusés), `OwlBrown 55`, `OwlPale 56`, `FoxRed 57`, `FoxPale 58`, `FoxDark 59`, `VoleBrown 60`, `Litter 61`. Les entrées 62 et 63 sont libres.
- **Police** : `Œ` et `œ` existent.
- **Sauvegarde** : version 7.

## 3. Registre des identifiants partagés

### 3.1 Graines (`seed ^ K`)
anomalies `0xa40a` · marais (génération) `0x3a75` · milieu du marais `0xb2b2` · météo `0x3a7f` · litière (hachage) `0x1177` · campagnols `0x701e` · renards `0xf0c5` · chouettes `0x0071` · constructions `0xb11d` · semis du joueur `0x5e3d` · la source `0x5005`.

### 3.2 `Matter` (étiquettes de sauvegarde)
`Litter = 20` (B2) · `Reed = 21` (A2) · `Pellet = 22`, `Feather = 23` (D1) · `RoastedHazelnut = 24`, `LeachedAcorn = 25`, `CookedMushroom { spotted } = 26` (D2).

### 3.3 `notebook::Entry` (étiquettes)
`Unkept(..)` 13 (A1 : une nuit d'anomalie sans anomalie) · `Lost(Spell)` 14 (A1) · `OwlGaze` 15, `Pellets` 16, `Silence` 17, `Mousing` 18, `Runways` 19 (D1) · `AlgaeRings` 20, `Pulse` 21, `Spiral` 22 (E1) · `Passage` 23 (E1, la fin). B1 possède le reste de `notebook.rs` et le format de `Entry` ; les autres lots ajoutent leurs variantes avec ces étiquettes.

### 3.4 Sons
`sound::AnimalCall` : `OwlHoot = 3`, `OwlKewick = 4`, `FoxBark = 5`, `FoxScream = 6`, `Rustle = 7` (D1) · `MarshPulse = 8`, `SourceHum = 9` (E1). `sound::Surface::Leaves = 4` (B2). Chaque nouvelle valeur reçoit un bras explicite **avant** `_` dans la synthèse (sinon elle joue le brame ou l'éclaboussure).

### 3.5 Touches (physiques, jouées en AZERTY)
Déjà prises : ZQSD/WASD, flèches, Maj, Espace, Tab/I, Échap, X, T, R, E, F, P, G, B, C, N, V, 1–8. **V maintenu = roue des sorts** (C1). **H = bâtir, Maj+H = bâtir à niveau** (D2). **J = enterrer une graine** (D2). Dans le carnet ouvert : ←/→ tournent les pages, la carte est une page virtuelle après la dernière (B1).

### 3.6 Ordre de sauvegarde (fin de `GameState::save`, après `now`)
1. `anomalies` (A1) ; les compteurs par joueur vont dans `PlayerState`.
2. (B2 : la litière est dans `Soil::save`, après `cover_start`.)
3. (B1 : les relèvements de regard sont dans `Notebook`.)
4. `voles`, `foxes`, `owls` (D1).
5. `structures`, `sown` (D2).
6. `marsh_medium` (E1), puis `source` (E1).

Le descripteur du marais n'est pas sauvegardé : il est recalculé à chaque lancement par la génération.

### 3.7 Drapeaux de capture et variables d'environnement
`--lost deer` (A1) · `--page N`, `--map`, `--forms N` (B1) · `--wheel`, `--owl` (C1) · `--marsh` (A2 : place le regard sur le marais) · `--demo-build`, `--demo-cook` (D2) · `--wave` (E1). `DISSIPATIF_DEBUG_ANOMALIES=1` (A1 ; chaque lot y ajoute ses lignes), `DISSIPATIF_DEBUG_DEER=1` (existe).

## 4. Les lots

Ordre par vagues de deux lots en parallèle : **A** (A1 ‖ A2), **B** (B1 ‖ B2), **C** (C1 ‖ C2), **D** (D1 ‖ D2), **E** (E1). Chaque vague part de `phase-2` après la fusion de la précédente.

### A1. Le cadre des anomalies, et le rituel des cerfs dedans

Nouveau module `crates/game/src/anomaly.rs`, état dans `GameState.anomalies`.

- `enum Kind { DeerRite }` (les lots suivants ajoutent `OwlGaze`, `MarshSpiral`), `enum Life { Dormant, Alive, Dead }`, `struct Anomaly { kind, site: Vec2, radius, life, health, strength, since, healthy_for, encore: Option<u32>, last_shown }`.
- **Santé du lieu** (long terme) : une mesure brute 0..1 par sorte, prise toutes les 30 s de jeu ; lissée en exponentielle (τ ≈ 1,5 jour) ; naissance si `health ≥ 0,65` pendant au moins `D` jours (rite : 2) ; mort si `health < 0,35` ; renaissance par la même règle de naissance (hystérésis). Rite : `min(prairie, harde)`, avec prairie = moyenne, sur les parcelles de sol à moins de 16 cases de l'anneau, de `min(1, cover/cover_start) · (1 − bareness)`, et harde = au moins 4 cerfs, réserves moyennes ≥ 0,4 (rampes douces). Au départ, le monde est à l'équilibre : l'anomalie est créée vivante, santé mesurée.
- **Force** (court terme) : 0..1 ; baisse quand on la dérange (rite : alarme ou fuite de la harde pendant la veille ou le rite −0,35 ; feu dans le rayon −0,25 par minute de jeu ; creuser dans le rayon −0,03 par poignée) ; remonte de 0,5 par jour × santé. Une anomalie faible se montre moins : moins de cerfs au cercle (`ceil(force · n)`), lueur plafonnée à `min(1, 0,4 + force)`, présages plus rares.
- **Moment** : chaque sorte a ses présages et sa fenêtre. **Tolérance** : si la fenêtre passe sans que l'anomalie aille à son sommet (rite : la lueur n'a jamais dépassé 0,6, ou la harde a fui), elle revient **une** fois la nuit suivante, aux mêmes heures, plus faible (force × 0,6). La harde ne décide plus seule du rite : `Herd::mode` lit un ordre posé par l'anomalie (rite ce soir ou non, présage ou non, force).
- **Épreuve et compréhension** généralisées : progression par joueur et par sorte (remplace `witnessed`) ; `GameState::understanding(id)` rend la sorte, la progression et le lieu (pour les lumières de `magic()`). Le rite garde son épreuve actuelle (10 s de jeu sans être vu pendant que la lueur dépasse 0,6, compris au sommet).
- **Sorts** : `Spell::OwlEye` ajouté (nom « Œil de chouette »). `PlayerState.spells` reste la liste des sorts **appris** ; un sort est **utilisable** si son anomalie source est vivante : `GameState::spell_state(id, spell) -> SpellState { Unknown, Usable, Lost }`. À la mort de l'anomalie : forme quittée si elle était prise, `Event::SpellLost { player, spell }` et `Entry::Lost(spell)` ; à la renaissance : `Event::SpellRegained`. `Command::Cast` refuse un sort perdu (`Failure::SpellLost`). Compteur monotone `PlayerState.transformations` (chaque sort appris la première fois l'augmente, jamais il ne baisse) : B1 en tire l'effacement des mots.
- API pour les lots suivants : `Anomalies::register(Anomaly)`, `disturb(kind, amount)`, `get(kind)`, une fonction `learn(player, spell, …)` commune (page, événement, compteur), et un point d'extension clair (`match kind`) pour la mesure de santé et le moment de chaque sorte.
- Outils : `DISSIPATIF_DEBUG_ANOMALIES=1` (sorte, vie, santé, force, lieu, encore), `--lost deer` (force la mort du rite pour voir le sort perdu), `GameState::force_life(kind, Life)` réservé aux tests et captures.
- Tests : naissance, mort, hystérésis, force et récupération, nuit de rattrapage ; le rite enseigne toujours (le test existant passe, en temps réel et accéléré) ; brûler la prairie tue le rite, retire le sort et rend forme humaine ; la renaissance rend le sort sans rendre de mots ; sauvegarde et reprise exactes.

### A2. Le marais dans le monde ; la plomberie de rendu des effets ; l'outillage de l'exercice

- **Monde** (`crates/world/src/marsh.rs`) : `World::make_marsh(near: [f32; 2], seed) -> Option<Marsh>`, appelé dans la génération du jeu juste après `deer::wear_ring` (avant `GameState::new`, `SceneData::build`, `Ambient::new`). Lieu : à 20–60 cases de l'anneau, à plus de 10 cases de lui, sol plat, Plaines ou Forêt, au plus bas localement, hors rivière et mer ; une cuvette de ~16 à 26 cases × 10 à 18 (forme par hachage), fond de `Mud`, profondeur 0,3–0,7, niveau sous le rebord (pas de mur d'eau flottant), `ground == tops` sous l'eau pour que l'eau se dessine. Plantes de la cuvette retirées (`unstamp_plant` puis `retain`, comme `wear`) ; roseaux ajoutés **en fin** de liste sur les bords et les hauts-fonds. `Marsh { centre, level, origin, size, cells }` (cellules triées), déterministe, non sauvegardé. Tests : un marais pour les graines 1 à 6, à la bonne distance, plat, rebord au-dessus du niveau, colonnes pleines jusqu'à leur sommet, roseaux présents, même graine même marais.
- **Roseau** : `Plant::Reed` en fin de `Plant::ALL`, modèle à la manière de `grass()` (tiges hautes de `Reed`, plumet de `ReedHead`, résolution `FINE`), couvre-sol (`is_ground_cover`) ; côté jeu : bras de `flexibility`/`yielding`, statique en écologie (`species` → `None`), récolte `Matter::Reed` (souple, fibreux, inflammable : bon chaume), modèle d'objet et `all_matters`.
- **Eau du marais** : `SceneData` garde un champ de vie de l'eau ; lentilles d'eau et algues en taches (plus denses vers les bords), palette dédiée dans `palette.rs` (testée), eau du marais plus sombre et trouble.
- **Plomberie des effets** (pour B2, C2, E1) : au groupe 0 de `voxel.wgsl`, un uniforme `Effects` (binding 6, VERTEX_FRAGMENT, `vec4` documentés et testés : `marsh` = origine x, z, texels par case, force ; `marsh_size` ; `mist` = centre x, z, rayon, quantité ; `wave` = origine x, z, phase, largeur ; `wave_style` ; `vision` ; deux `vec4` libres) et deux textures R32 non filtrables lues par `textureLoad` : binding 7 `litter_field` (une valeur par parcelle de sol 8 × 8, `nx/8 × 1 × nz/8`) et binding 8 `marsh_field` (grille du milieu du marais, taille fixe 128 × 1 × 128, rangée en sous-région). API : `Renderer::set_effects(&EffectsUniform)`, `upload_litter(&Field3)`, `upload_marsh(&Field3)`, toutes sans effet visible tant qu'on n'y écrit rien. Attention : les pipelines d'ombre, de particules et de ciel ont leurs propres mises en page ; ne lire ces bindings que là où ils existent ; garder `haze()` sans ressource du groupe 1. Ajouter aussi `FieldTexture::upload_region` (écriture d'une sous-boîte) pour éviter les 67 Mo de `surface_changed` (D2 s'en servira).
- **Outillage de l'exercice** (`crates/sim`, `crates/lab`) : tests `#[ignore = "exercice : step de l'Oregonator"]` en plus de l'existant (diffusion seule symétrique et conservée avec bords étanches ; long calcul fini et borné ; onde à vitesse constante ; une onde ne repart pas en arrière (réfractaire) ; une spirale tient en 2D en régime excitable) ; un accesseur sans calcul `Oregonator::set_cell(x, z, u, v)` et `fields()` pour amorcer une spirale et sauvegarder ; `lab bz-wave` et `lab bz-spiral` (CSV et images PGM dans `crates/lab/results/`), qui attrapent la panique de `todo!()` et disent « exercice non écrit ». Ne pas écrire `step`.
- Capture : `--marsh` place le regard sur le marais ; `DISSIPATIF_DEBUG_ANOMALIES` affiche son centre.

### B1. Le carnet qui perd ses mots

- **Primitives d'interface** (`render/src/ui.rs`) : `triangle`, `line`, `disc`, `sector` (anneaux et parts), `bitmap`, `text_eroded(x, y, text, scale, color, erosion, seed)` ; testées (sommets dans leur boîte, déterminisme, érosion 0 = `text`).
- **Signes** (`crates/game/src/signs.rs`, partagé avec la roue de C1) : la lune (partie éclairée = `moon_light`, testé), le soleil sur son arc (heure), le vent en flèche nord en haut, une petite carte (point de départ, lieu de la page, `world.nearest`), et un signe par sort (`spell_sign(ui, spell, centre, size, ink, state)` : bois de cerf, œil de chouette ; un sort perdu est grisé **et** fissuré).
- **Pages** : l'en-tête de mots (jour, heure, lune, vent, lieu) devient une rangée de signes. Les mots des pages s'effacent selon `transformations` (A1) : lexique de concrétude (mots du temps, verbes, mots-outils d'abord ; noms concrets — arbre, cerf, chouette, eau, terre — en dernier) ; les lettres se défont puis le mot devient un griffonnage ; graine par (page, ligne, mot) ; irréversible. Le titre d'un sort perdu reste illisible, son signe fissuré.
- **Liens** : « même lieu » (≤ 12 cases, distance sur le tore) et « même lune » (même octant de phase), dessinés en petits signes avec le numéro de page (fonction pure `links`, testée).
- **Relèvements** : `Bearing { from, toward, day, hour, who }` dans `Notebook`, `note_bearing(b) -> bool` (une fois par oiseau et par nuit) ; la **carte** est une double page virtuelle après la dernière : nord en haut, centrée sur le lieu de découverte du carnet, chaque relèvement tracé en pointillés depuis son point d'observation (découpés au cadre) ; c'est là qu'on croise les regards.
- Capture : `--page N`, `--map`, `--forms N`.

### B2. Les feuilles qui tombent, la litière qui devient humus

- `season::leaf_fall(year)` : chute par jour, qui **commence à la fin de l'été** (premières feuilles dès le début de la tranche), culmine au milieu de l'automne, finit au début de l'hiver ; et un léger avancement du jaunissement (`look().autumn`) pour que la tranche montre les premières feuilles jaunes. Garder les tests de saison cohérents.
- `Soil` : un stock de litière par parcelle (kg/m²), apport depuis les houppiers caducs (chênes, bouleaux, noisetiers, feuillus, saules, buissons ; pas les pins ni les palmiers) calculé dans `Ecology::measure_cover`, décomposition `k(T, W) · L` (Q10 ≈ 2, demi-vie d'environ une à deux années de jeu de 64 jours), une part devient humus. **Garder l'invariant de départ** : après `settle()`, l'offre de chaque parcelle vaut 1 (le monde généré est à l'équilibre) et les tests d'hystérésis du sol passent ; l'effet de la litière sur l'humus reste modéré.
- Ramasser : `Matter::Litter` (« Feuilles mortes », léger, très inflammable : un bon amadou), dans `Pick` et `PickAt` après les plantes et avant la poignée de terre, quand la parcelle en a assez ; le stock baisse.
- Bruit : `observer().ground_noise` monte avec la litière (les cerfs, et plus tard les chouettes, entendent mieux) ; `Surface::Leaves` (pas qui crissent) ; envol de feuilles sous les pas (`traces.rs`).
- Visible : les feuilles qui tombent (`ambient.rs`) suivent `leaf_fall` et le vent, prennent les couleurs de la saison, ne viennent que des houppiers caducs, reposent un instant au sol, et respectent le tore ; le tapis au sol est lu dans `litter_field` (binding 7, A2) par `fs_main` sur les faces du dessus du sol, mêlé à la couleur des feuilles mortes selon la quantité, par taches. Téléversé quand il change (cadence de `step_ground`).
- `Soil::litter(k)` public (E1 l'utilise pour les feuilles qui tombent dans le marais). Fiche `docs/reactions/ecologie.md` mise à jour.

### C1. La roue des sorts et l'Œil de chouette

- **Roue** : V maintenu ouvre la roue au centre de l'écran (les sorts appris, dessinés par `signs::spell_sign`, les perdus fissurés et non choisissables) ; on vise à la souris ; relâcher lance le sort visé, relâcher au centre annule ; un appui bref sur V en forme de cerf rend forme humaine. Fonction pure `wheel_slot_at` partagée entre dessin et clic, testée. Clics bloqués pendant que la roue est ouverte, fermée si la fenêtre perd le focus. Messages : plus de « V pour le lancer ».
- **Œil de chouette** (sort de perception qu'on ouvre et ferme ; utilisable aussi en forme de cerf) : `owl_eye` sauvegardé avec le joueur ; vision de nuit en post-traitement (`PostUniform` agrandi d'un `vec4 vision`, testé) ; **éblouissement** par les feux proches ou en vue (blanc, puis une gêne qui s'efface en quelques secondes) ; **ouïe** : les cris portent plus loin, et ce qui bouge s'entend : `GameState::movers()` (cerfs, écureuils ; D1 y ajoutera renards, campagnols, chouettes) donne des lueurs douces aux bêtes qui bougent la nuit.
- Capture : `--wheel`, `--owl`.

### C2. Des saisons qui comptent : lumière, météo, sons

- **Météo tirée de la graine** : averses et brumes en fonctions pures des jours de jeu (`hash_unit(seed ^ 0x3a7f, [créneau])`), plus d'averses à l'automne ; `Clock::conditions` les fournit, `Weather` ne garde que l'aspect (gouttes, humidité du sol qui suit) ; même résultat accéléré ou non ; R reste un forçage de développement.
- **Durée du jour** : `season::daylight(year)` (lever, coucher, hauteur du soleil à midi) ; `sky::sky_on(hour, daylight)` (avec `sky(hour)` gardé pour les tests) ; tous les appelants passent à la version saisonnière, y compris `deer::daylight` (gameplay), l'écoute et les heures des écureuils. Test de continuité sur l'année.
- **Lumière d'automne** : soleil plus bas, plus doré, heures dorées plus longues.
- **Brume du matin sur le marais** : brume locale dans `haze()` par l'uniforme `Effects.mist` (A2), à l'aube, plus forte à l'automne et après une nuit fraîche et humide.
- **Sons** : les grillons s'éteignent peu à peu (année et température, chant plus lent quand il fait frais) ; les lucioles s'éteignent en douceur ; le chœur de l'aube suit le lever du soleil.

### D1. Chouettes, renard, campagnols ; le regard des chouettes

- **Perception commune** (`crates/game/src/perception.rs`) : la vue, l'ouïe et l'odorat de `Herd::sense`, factorisés avec des portées par espèce, réutilisés par les cerfs (sans changer leur comportement : les tests du cerf passent tels quels), le renard et les chouettes (ouïe extrême).
- **Campagnols** : densité par parcelle de sol (croissance logistique lente, capacité selon l'herbe, prédation), galeries visibles quand ils sont nombreux (indice), quelques individus montrés près du joueur (présentation, à la manière des poissons de `fauna.rs`), bruissements (`Rustle`) entendus avec l'Œil de chouette.
- **Renard** (1 ou 2, `GameState`) : terrier en lisière ; actif du crépuscule à l'aube ; trotte dans les prairies ; **mulotage** (s'arrête, écoute, bond en arc sur une parcelle riche, la densité baisse) ; aboiements et cris d'automne la nuit ; traces sauvegardées, dessinées en petites marques sur sol meuble ; fuit le joueur.
- **Chouettes hulottes** (3, `GameState`) : territoires sur des arbres de lisière autour du marais (25–60 cases, espacés) ; cachées le jour ; la nuit, perchées, duos (hou-hou et kiwitt), chasse à l'affût sur les parcelles riches en campagnols, vol silencieux entre perchoirs ; tête qui tourne (partie séparée du modèle).
- **L'anomalie `Kind::OwlGaze`** (cadre A1) : santé = au moins deux chouettes et assez de campagnols autour du marais ; moment = nuits sombres (lumière de lune < 0,35), de 23 h à 2 h ; présages = la chaîne de hululements qui mène vers le point (dès 21 h les nuits presque sombres) ; pendant le regard, chaque chouette se pose sur l'arbre de son territoire le plus proche du point, **tête tournée vers lui**, immobile ; indices = pelotes et plumes (`Matter::Pellet`, `Feather`, peu nombreuses) tombées sous les perchoirs **du côté du point**, le matin d'après ; **silence** : toutes se taisent au même instant si le joueur approche du point ou se fait entendre. Point regardé = centre du marais (E1 le fera dériver). Relèvements : observer une chouette qui regarde (à moins de 14 cases, la nuit) écrit un relèvement (`note_bearing`) et la page `OwlGaze` la première fois.
- **Épreuve** : se tenir à moins de 2,5 cases du point pendant le regard, 12 s de jeu sans être entendu des chouettes → `Spell::OwlEye` (par `learn` de A1). Entendu : elles s'envolent, force −0,3, la nuit est manquée (rattrapage la nuit suivante). La Forme du cerf aide (pas de cerf, presque silencieux) mais l'épreuve est faisable sans elle (accroupi, lentement, sans feuilles sous les pieds, depuis un affût).
- `movers()` étendu ; sons `OwlHoot`, `OwlKewick`, `FoxBark`, `FoxScream`, `Rustle` (synthèse, tests « entendu puis fini », démo) ; réserve de cris agrandie et sans collisions (file circulaire d'atomiques) ; modèles (`owl_view.rs`, `fox_view.rs`, campagnols) ; notes de carnet (`Pellets`, `Silence`, `Mousing`, `Runways`).

### D2. La vie quotidienne : bâtir, l'affût, l'abri, cuisiner, cueillir, semer

- **Bâtir** sans recette, par les propriétés, sur la grille : `World::set_block` (seulement des matériaux construits, au-dessus du sol, sur de l'air ou un autre construit, sur le tore), `SceneData::block_changed` (occupation de la colonne, remaillage du tronçon, couleur par `upload_region`), événements `Built` et `Unbuilt`. H pose à la colonne visée, au-dessus du sol ou de la pile ; Maj+H pose au niveau du haut de la pièce voisine (pour un plancher ou un toit). La nature de la pièce découle de la matière choisie et du voisinage : bois sec et raide (baguettes, bâtons) soutenu par en dessous → **poteau** (`Timber`) ; bois raide sans appui dessous mais à côté d'un poteau ou d'un plancher → **plancher** (`Plank`) ; bois vert et souple à côté d'un poteau → **clayonnage** (`Wattle`) ; argile sur un clayonnage → **torchis** (`Daub`) ; herbe, fougère ou roseau au-dessus d'un mur ou d'un poteau → **chaume** (`Thatch`). Chaque pièce consomme quelques objets du sac.
- **Appuis** : poteau sur le sol ou sur un poteau ; plancher contre un poteau ; mur sur le sol ou contre un poteau ; toit contre un mur ou un poteau. Ce qui perd son appui tombe (quelques objets au sol). **Le feu** prend aux pièces de bois et de chaume (depuis les feux et les plantes en feu voisines), les consume, et la structure s'écroule ; le bois et le chaume **pourrissent** en quelques années de jeu. État par pièce dans `GameState.structures` (sauvegardé).
- **Affût** : les pièces construites et les objets feuillus posés (fagots verts, fougères, feuilles mortes) autour du joueur augmentent sa couverture (`observer().cover`) ; entouré sur trois côtés, ses mouvements ne se voient plus ; l'odeur, elle, passe toujours (il faut être sous le vent).
- **Abri** : un toit au-dessus de la tête (moins de 4 cases) retire l'effet de la pluie ; des murs autour coupent le vent (+ quelques degrés) ; le feu dedans réchauffe (existe) ; la pluie ne tombe plus sous le toit (aspect).
- **Cuisiner** par la physique des objets : `History.cooked_seconds` (au-dessus de ~120 °C) et la carbonisation (au-dessus de ~300 °C) ; noisette grillée (plus nourrissante), champignon cuit (plus nourrissant ; un champignon tacheté reste toxique) ; **glands lessivés** : on peut poser de petites choses en eau peu profonde, un gland y perd ses tanins en ~1,5 jour de jeu (comptés en temps de jeu) ; une chose carbonisée devient cendre.
- **Cueillir** : stocks d'automne par arbre (noisettes dès la fin de l'été, glands à l'automne), ramassés sans lame ; les écureuils en prennent aussi.
- **Semer** : J enterre la graine choisie (gland, noisette) à ses pieds ; elle dort jusqu'au printemps (liste à part, hors du plafond de la banque de graines), puis tente sa germination.
- Capture : `--demo-build` (une petite cabane et un affût), `--demo-cook`.

### E1. La spirale du marais, et la fin

- **Milieu du marais** (`crates/game/src/marsh.rs`, dans `GameState`) : une grille (2 à 4 texels par case) sur la boîte du marais, masquée hors de l'eau. **En attendant l'exercice**, le milieu est un **automate excitable de Greenberg-Hastings** (repos, excité, réfractaire ; un voisin excité suffit au-dessus d'un seuil) écrit dans `crates/game` et présenté comme une doublure ; la fonctionnalité Cargo `oregonator` du jeu remplace la doublure par `sim::oregonator::Oregonator` (l'exercice de l'utilisateur) avec des paramètres excitables et un nombre de pas par seconde réglé par la température. Fiche `docs/reactions/milieu-excitable.md` (la doublure, ses limites, ce que l'Oregonator apporte).
- **Température** (loi d'Arrhenius) : la cadence de l'automate suit la température de l'eau (`needs::temperature` au marais) ; les nuits fraîches et l'automne ralentissent la spirale.
- **Santé `Kind::MarshSpiral`** : eau présente, **turbidité** basse (cendres des feux proches, terre creusée près du marais, sol mis à nu autour ; elle retombe en quelques jours), nutriments dans une fourchette (les feuilles mortes qui tombent dans l'eau en apportent ; trop, et l'eau verdit) ; une eau trouble rend le milieu moins excitable : les ondes s'éteignent, l'anomalie meurt ; elle peut renaître.
- **Naître et relancer** : la spirale existe au départ (amorcée par une onde rompue déterministe) ; poser un caillou dans l'eau du marais lance une onde en cible ; marcher dans l'eau rompt les fronts (de nouvelles spirales peuvent naître).
- **Voir et entendre** : de jour, des **anneaux d'algues** (la moyenne lente du motif, dans le vert des lentilles) ; la nuit, rien sans l'Œil de chouette ; avec lui, la spirale luit (palette de la réaction, lisible en luminance) et chaque front qui passe près du joueur pulse (`MarshPulse`). Pages `AlgaeRings`, `Pulse`, `Spiral`.
- **La source et la fin** : `source.rs`, fonctions pures du temps et de la graine : un grand cycle qui module la force de toutes les anomalies ; une onde lente qui traverse la planète dans une direction tirée de la graine ; le **point fixé par les chouettes dérive** nuit après nuit dans cette direction (D1 lit `source::gaze_point`). **La fin** : une nuit sombre, après avoir appris l'Œil de chouette et vu la spirale, près du marais avec l'Œil ouvert, l'onde passe : une bande de lumière lente traverse le paysage (`Effects.wave`), les bêtes se figent, les chouettes se taisent, la spirale pulse en phase (l'onde excite le milieu), un bourdonnement (`SourceHum`), une page faite seulement de signes (`Passage`) et le message « Quelque chose est passé. » ; le jeu continue, l'onde reviendra. Capture : `--wave`.

## 5. Fusion et relecture

Après chaque vague : relecture de chaque branche (bogues, déterminisme, sauvegarde, tore, performance, conventions), corrections dans la branche, puis fusion dans `phase-2`, `cargo test --workspace`, `clippy`, captures de contrôle. À la fin : une partie de six jours rejouée en captures (arrivée, présages, rite, automne, nuit sombre, marais, fin), puis mise à jour de `phase-2.md` (statuts), `roadmap.md` et `decisions.md`.
