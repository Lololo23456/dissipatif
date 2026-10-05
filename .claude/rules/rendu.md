---
paths:
  - "crates/render/**"
  - "crates/game/**"
---
# Règles pour le rendu et le jeu

## Tout est procédural
- Aucune image, texture ou modèle importé. Pas de fichiers .png, .jpg, .gltf dans le dépôt pour l'art du jeu.
- L'apparence de chaque voxel est une fonction de son état local (concentrations, température, énergie, âge). Si une couleur ne dépend d'aucun état, demande-toi pourquoi elle existe.
- Toutes les palettes et fonctions de couleur vivent dans un seul module (`crates/render/src/palette.rs`) pour garder une direction artistique cohérente. Palettes restreintes, choisies, pas d'arc-en-ciel par défaut.
- La lisibilité ne repose jamais sur la seule teinte : la luminance, le mouvement ou le son doivent aussi porter l'information (daltonisme).

## Architecture voxel
- Deux échelles : grille macro, et briques micro 4×4×4 allouées seulement pour les cubes subdivisés (structure de type brickmap).
- Cubes macro : maillage glouton (greedy meshing), remaillage par chunk seulement quand un chunk change.
- Micro-cubes et particules : instancing, mis à jour par compute shader. Chaque particule garde la quantité de matière qu'elle porte, même si elle est d'abord décorative.
- Caméra plongeante type Minecraft Dungeons, zones limitées : pas de système de niveau de détail lointain tant que ce n'est pas nécessaire.

## wgpu
- Version 30 épinglée. Ne devine pas une signature : vérifie dans les sources de la crate ou docs.rs/wgpu/30.
- Explique brièvement chaque nouveau concept wgpu la première fois qu'il apparaît dans le projet (bind group, pipeline layout, etc.).
- Le rendu lit l'état de la simulation, il ne le modifie jamais.
