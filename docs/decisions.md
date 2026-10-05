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

## 2026-10-05 — Socle de rendu minimal en parallèle de la simulation
Précise « Construire la simulation avant le moteur voxel » : on construit seulement ce que le prototype exige pour afficher une parcelle (wgpu, caméra plongeante, maillage des cubes macro, couleur, éclairage simple), testé sur un champ synthétique en attendant Gray-Scott. Briques micro 4×4×4, particules et instancing attendent la récolte. **Actée.**

## 2026-10-05 — Géométrie par occupation, couleur par texture 3D
La couleur de chaque voxel dépend d'un état continu : un maillage glouton qui fusionne par couleur ne fusionnerait presque rien. Le maillage ne dépend que de l'occupation (seuil sur le champ) ; le fragment shader lit le champ dans une texture 3D et applique la palette. Changer les couleurs coûte un envoi de texture, on ne remaille que si l'occupation change. On commence par un maillage des faces visibles (culled) ; le glouton seulement si les mesures le justifient. Mesure : 48³ maillé en 0,34 ms (7 900 faces), le glouton n'est pas nécessaire. **Provisoire.**

## 2026-10-05 — La forme et la couleur viennent de deux champs différents
Sur la surface visible de {champ > seuil}, le champ vaut presque partout le seuil (mesuré : 50 % des faces entre 0,500 et 0,525 pour un seuil de 0,5). Colorier une forme par le champ qui la définit ne montre rien. Avec Gray-Scott : la forme vient de v, la couleur d'un autre état (u, température, âge…). **Provisoire.**

## 2026-10-05 — Direction artistique : lumière dorée, référence Firewatch
Aplats de couleur, palette chaude restreinte (prune → or pâle), soleil chaud et ombres froides éclairées par le ciel (ambiante hémisphérique), occlusion ambiante par sommet, brume pêche avec la distance qui sert aussi de fond. La palette des concentrations reste dans L* ≈ 36 → 75 pour laisser la luminance à l'éclairage, qui porte la forme. Tout vit dans `palette.rs`. Répond en partie à « Univers et ambiance » de `design.md`. **Provisoire.**

## 2026-10-05 — Un terrain fixe et un bassin de mélange
Une réaction qui occupe tout le volume est illisible : aucun repère fixe, et la caméra ne voit que la surface d'un bloc. Le monde est un terrain inerte, généré une fois, et la réaction tourne dans un bassin peu profond creusé dedans (40×6×40), à parois étanches (Neumann, flux nul). La terre ne participe pas à la réaction pour l'instant ; la nourriture vient toujours du réservoir F. Côté rendu, terrain et bassin sont deux « volumes » (maillage, champ de couleur, palette, origine) dessinés par le même pipeline. **Remplacée** le jour même par « La terre est vivante » : le bassin restait un objet collé dans le décor.

## 2026-10-05 — La terre est vivante
La réaction ne vit plus dans un bassin séparé : elle tourne dans la couche de surface du terrain (grille 2D de 72×72, une cellule par colonne, parois étanches aux bords du monde) et se lit dans la couleur du sol. Le terrain ne change jamais de forme : maillé une fois, seul un champ « vie » est envoyé au GPU à chaque image. Le shader mélange la couleur de la terre (palette `earth`) et celle du vivant (palette `concentration`) selon v : terre teintée au bord d'une tache, or au cœur, plus clair que la terre (lisible sans la teinte). Limites assumées pour l'instant : la diffusion ignore le relief, et la terre ne nourrit pas la réaction (F uniforme). **Provisoire.**
