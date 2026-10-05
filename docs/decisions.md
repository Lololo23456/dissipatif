# Journal des décisions

Une entrée par décision. Statut : **actée** ou **provisoire** (à revoir quand le prototype en dira plus).
Format : date, décision, raison, statut.

## 2026-10-05 — Rust + wgpu, sans moteur de jeu
Contrôle total sur les compute shaders, la simulation est le cœur du projet. **Actée.**

## 2026-10-05 — Simulation d'abord sur CPU, GPU ensuite
Un bug numérique se trouve bien plus facilement en Rust qu'en WGSL. La version CPU reste la référence de vérité pour valider la version GPU. **Provisoire.**

## 2026-10-05 — Construire la simulation avant le moteur voxel
Le premier prototype doit prouver que la mécanique est prenante avant d'investir dans le rendu. **Actée.**

## 2026-10-05 — Style Minecraft Dungeons, cubes subdivisibles
Caméra plongeante et zones limitées : moins de distance d'affichage, plus de budget pour la simulation. Cubes macro subdivisibles en micro-cubes pour les particules. **Actée.**

## 2026-10-05 — Facteur de subdivision 4×4×4
64 micro-cubes par cube : assez pour des éclats lisibles, mémoire raisonnable. Passer à 8×8×8 seulement si le rendu le justifie. **Provisoire.**

## 2026-10-05 — Particules visuelles au départ
Les micro-cubes sont d'abord décoratifs. Les structures de données gardent la quantité de matière portée par chaque particule pour permettre plus tard la conservation de la matière. **Provisoire.**

## 2026-10-05 — Claude explique, l'humain écrit le cœur de la simulation
Objectif d'apprentissage. Dans `crates/sim` et les compute shaders, Claude explique et relit ; il n'écrit l'implémentation que sur demande explicite. **Provisoire.**

## 2026-10-05 — Versions épinglées
wgpu 30, winit 0.30, glam 0.34, bytemuck 1, pollster 1. Une montée de version majeure de wgpu se fait dans un commit dédié. **Actée.**

## 2026-10-05 — Nom « Dissipatif »
**Provisoire.**
