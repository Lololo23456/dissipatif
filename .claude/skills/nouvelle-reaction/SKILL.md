---
name: nouvelle-reaction
description: Ajoute une nouvelle règle de réaction ou un nouveau système dynamique au jeu, de l'équation jusqu'aux tests et à la fiche de documentation. À utiliser quand l'utilisateur veut ajouter une réaction chimique, un automate cellulaire, un modèle de réaction-diffusion, un oscillateur (Brusselator, Belousov-Zhabotinsky, FitzHugh-Nagumo, Lenia…) ou tout nouveau type de culture pour les parcelles.
disable-model-invocation: true
argument-hint: [nom-du-modèle]
---

# Ajouter la règle de réaction « $ARGUMENTS »

Suis ces étapes dans l'ordre. Respecte le mode pédagogique de `.claude/rules/simulation.md` : l'utilisateur écrit le cœur numérique, toi tu expliques, structures, testes et relis.

## 1. Comprendre le modèle
Présente le modèle à l'utilisateur :
- les variables (espèces, champs) et leur sens physique ;
- les équations, écrites proprement ;
- l'origine du modèle et le phénomène réel qu'il décrit ;
- ce que chaque paramètre deviendra dans le jeu (flux, dissipation, température…).
Demande à l'utilisateur s'il veut qu'on adapte le modèle avant d'aller plus loin.

## 2. Analyse avant le code
- États stationnaires homogènes et, si c'est faisable, leur stabilité linéaire (jacobienne, valeurs propres). Cherche les courbes de bifurcation connues dans la littérature.
- Condition de stabilité du schéma explicite : D·dt ≤ 1/6 pour la diffusion en 3D, plus une estimation pour les termes de réaction.
- Ordres de grandeur des paramètres où des motifs apparaissent.

## 3. Fiche de documentation
Crée `docs/reactions/$ARGUMENTS.md` sur le modèle de `docs/reactions/gray-scott.md` : équations, tableau des paramètres (sens physique et sens dans le jeu), stabilité numérique, états stationnaires, tableau des régimes (vide pour l'instant).

## 4. Structure du code
Dans `crates/sim`, propose l'emplacement et la signature (struct des paramètres avec des noms explicites, implémentation du trait de règle de réaction s'il existe). Écris les structures vides et les signatures, puis laisse l'utilisateur écrire le calcul du pas de temps. Écris-le toi-même seulement s'il le demande explicitement.

## 5. Tests
Écris toi-même les tests dans `crates/sim` (voir la liste dans `.claude/rules/simulation.md`) : état homogène stationnaire, déterminisme, absence d'explosion sur un long run, conservation si applicable. Lance `cargo test -p sim`.

## 6. Relecture
Quand l'utilisateur a écrit le calcul, relis-le : stabilité, double tampon, bords, performance. Pour un modèle complexe, propose le subagent `relecteur-physique`.

## 7. Premiers régimes
Propose de lancer la skill `balayage-parametres` pour remplir le tableau des régimes de la fiche.

## 8. Journal
Ajoute une entrée dans `docs/decisions.md` si le modèle a demandé un choix d'architecture (nouveau trait, nouveau type de grille, etc.).
