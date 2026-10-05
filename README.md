# Dissipatif (nom provisoire)

Jeu voxel de gestion et d'exploration où l'on cultive des structures dissipatives. Vision et mécaniques : `docs/design.md`.

## Installation

1. Installer Rust (édition 2024, Rust 1.85 ou plus récent) avec rustup.
2. Installer le validateur de shaders : `cargo install naga-cli --version 30.0.1`
3. Recommandé : installer `jq` (les hooks Claude Code se rabattent sur python3 sinon).
4. `git init && git add . && git commit -m "Squelette du projet"`
5. `cargo test --workspace` puis `cargo run -p game` : une fenêtre vide doit s'ouvrir.

## Travailler avec Claude Code

La configuration est dans `CLAUDE.md` et `.claude/`. Au premier lancement de `claude` dans ce dossier, valide les hooks du projet.

- `/explique <concept>` : comprendre un concept ou un bout de code.
- `/nouvelle-reaction <nom>` : ajouter une règle de réaction de A à Z.
- `/balayage-parametres <règle> …` : cartographier les régimes d'une règle.
- « Utilise le subagent relecteur-physique » après une modification de la simulation.
- `/context` pour vérifier ce qui est chargé.
