# Dissipatif (nom provisoire)

Jeu voxel d'exploration contemplative : un naturaliste vu de haut observe et cherche à comprendre
un monde procédural très vivant (faune, flore animée, jour et nuit, ciel étoilé), où existe une autre
forme de vie, les anomalies : des structures dissipatives (réaction-diffusion, systèmes loin de l'équilibre).
Priorité actuelle : l'ambiance. Tout l'art est procédural : aucune texture, aucun asset dessiné,
l'apparence découle de l'état du monde.
Le game design complet est dans `docs/design.md` : lis-le seulement quand une tâche touche au gameplay.
Les décisions passées et les choix encore provisoires sont dans `docs/decisions.md`. Consulte-le avant de remettre en cause un choix d'architecture.

## Stack (versions épinglées, ne pas changer sans décision dans docs/decisions.md)
- Rust, édition 2024, workspace Cargo
- wgpu 30.x (shaders en WGSL), winit 0.30.x, glam 0.34.x, bytemuck 1.x, pollster 1.x
- L'API de wgpu change souvent entre versions majeures : utilise l'API de la version 30, vérifie dans `~/.cargo/registry` ou docs.rs/wgpu/30 en cas de doute, n'invente pas de signature.

## Organisation du workspace
- `crates/sim` : simulation pure, sans GPU ni fenêtre. Grilles, règles de réaction, intégrateurs, métriques. Référence de vérité.
- `crates/world` : génération procédurale du monde, pure et déterministe : relief, climat, biomes, lacs et rivières, végétation. Sortie : voxels de matériaux et niveaux d'eau.
- `crates/render` : tout ce qui touche wgpu : pipelines, buffers, shaders (`crates/render/shaders/*.wgsl`), maillage, particules.
- `crates/game` : binaire jouable. winit, entrées, caméra, logique de jeu, liaison sim ↔ rendu.
- `crates/lab` : binaire sans affichage pour les expériences : balayages de paramètres, mesures, export CSV.
- `docs/reactions/` : une fiche par règle de réaction (équations, paramètres, analyse de stabilité, régimes connus).

## Commandes
- Vérifier : `cargo check --workspace`
- Tests : `cargo test --workspace` ; un seul crate : `cargo test -p sim`
- Lancer le jeu : `cargo run --release -p game` (`-- --seed N` pour un autre monde)
- Capture sans fenêtre : `cargo run --release -p game -- --capture vue.png [--seed N] [--at X,Z] [--zoom F] [--yaw DEG] [--pitch DEG] [--size LxH]`. À utiliser pour vérifier un rendu : l'image se lit avec l'outil Read.
- Expériences : `cargo run --release -p lab -- <sous-commande>`
- Lint : `cargo clippy --workspace --all-targets -- -D warnings`
- Format : `cargo fmt --all` (fait automatiquement par un hook après chaque modification)
- Valider un shader : `naga crates/render/shaders/<fichier>.wgsl` (fait automatiquement par un hook)

## Conventions
- Code, identifiants et commentaires de code en anglais. Docs, fiches et explications en français.
- La simulation tourne d'abord sur CPU (`crates/sim`). Une version GPU (compute shader) n'est écrite qu'une fois la version CPU testée, et doit être validée contre elle.
- Simulation déterministe : toute aléa passe par un générateur à graine explicite. Même graine, mêmes paramètres → même résultat sur CPU.
- Mise à jour par double tampon (ping-pong) : jamais de mise à jour en place d'une grille pendant un pas.
- Monde à deux échelles : grille macro (gameplay et simulation) et briques micro de 4×4×4 par cube, allouées seulement quand un cube est subdivisé (particules, récolte, destruction).
- Rendu : caméra plongeante type Minecraft Dungeons, zones de taille limitée. Maillage glouton pour les cubes macro, instancing pour les micro-cubes.
- Pas de `unwrap()` dans le code de jeu hors initialisation ; pas d'allocation dans les boucles de simulation.

## Façon de travailler avec moi
- Je suis étudiant et je veux comprendre le cœur de la simulation. Dans `crates/sim` et les compute shaders, suis la règle `.claude/rules/simulation.md` : explique et relis, n'écris pas l'implémentation à ma place sauf si je le demande explicitement.
- Ailleurs (plomberie wgpu, fenêtre, outillage, tests, lab), tu peux écrire directement.
- Pour une tâche non triviale : explore et propose un plan avant de coder.
- Après une modification de la simulation, lance `cargo test -p sim`. Pour une modification numérique importante, propose le subagent `relecteur-physique`.

## Pièges
- Alignement mémoire Rust ↔ WGSL : voir `.claude/rules/wgsl.md`. Un mauvais padding ne plante pas, il donne des valeurs fausses.
- Stabilité numérique : Euler explicite + laplacien 7 points en 3D exige D·dt ≤ 1/6 (avec dx = 1). Au-delà, la simulation explose silencieusement.
- Ne compare jamais un résultat GPU et CPU à l'égalité exacte : tolérance en flottants.
- Toute décision qui change l'architecture ou le design s'inscrit dans `docs/decisions.md`.
