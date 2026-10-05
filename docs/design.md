# Document de game design

Version de travail. Les choix encore ouverts sont marqués « à trancher » et suivis dans `decisions.md`.

## Vision

Un jeu de gestion et d'exploration où l'on cultive des structures qui n'existent que parce qu'un flux d'énergie et de matière les traverse. Le joueur ne contrôle jamais directement ce qui pousse : il règle les flux, et l'ordre émerge, ou s'effondre. Chaque mécanique est un vrai phénomène de la physique hors équilibre, si bien que jouer revient à développer une intuition de la thermodynamique de Prigogine.

## Piliers

1. **L'émergence est le gameplay.** Ce qui est beau et ce qui est utile sont la même chose : l'état de la simulation.
2. **Tout est généré.** Terrain, couleurs, matériaux, végétation, particules et son découlent d'équations. La direction artistique vit dans le choix des règles, des palettes et des paramètres.
3. **Le temps a une flèche.** Pas de retour en arrière gratuit : hystérésis, irréversibilité, mémoire des systèmes.
4. **Comprendre, c'est progresser.** La progression vient des découvertes et des instruments qui rendent visible ce qui était caché.

## Boucle centrale

Explorer → rapporter semences, règles et sources d'énergie → cultiver dans des parcelles → récolter → construire outils, régulateurs et extensions → explorer plus loin.

## Mécaniques

### Parcelles
Volume de voxels où tourne une règle de réaction. Le joueur agit sur le flux entrant, la dissipation, la température et peut injecter des perturbations locales. Selon les réglages émergent cellules, labyrinthes, spirales, oscillations, ou rien.

### Régimes et récolte
Chaque régime produit une ressource différente. Récolter retire de la matière, donc perturbe le système : une récolte trop avide fait basculer la parcelle.

### Bifurcations et ralentissement critique
Le rendement maximal se trouve près des points de bascule. À l'approche d'une bifurcation, le système récupère de plus en plus lentement des perturbations et ses fluctuations augmentent. Le jeu le rend perceptible : tremblement visuel, instabilité sonore.

### Hystérésis
Remettre un réglage à sa valeur précédente ne restaure pas l'état précédent. Il faut une poussée plus forte en sens inverse, ou recommencer.

### Énergie et entropie
Toute parcelle consomme un gradient d'énergie et produit de l'entropie (chaleur, déchets) qu'il faut évacuer. Une base qui n'évacue pas chauffe, et la chaleur fait dériver les régimes.

### Couplages
Canaux et membranes relient les parcelles : les déchets de l'une nourrissent l'autre. Les couplages propagent aussi l'instabilité ; les membranes servent de filtres et de coupe-feu.

### Régulateurs
Capteurs, actionneurs et liaisons pour construire des boucles de rétroaction. Un régulateur trop lent ou trop agressif crée lui-même des oscillations.

### Exploration
Régions sauvages où des régimes naturels tournent à grande échelle. On y trouve semences (motifs rares), nouvelles règles de réaction, gradients d'énergie pour des avant-postes, et dangers comme des fronts chaotiques qui avancent.

### Progression
Carnet des structures découvertes. Les découvertes débloquent des instruments : carte des régimes, détecteur de ralentissement critique, spectromètre des rythmes.

### Objectif à long terme
Créer une structure qui se maintient seule, sans régulateur, et idéalement qui produit de la nouveauté par elle-même : une proto-vie.

## Monde et rendu

- Caméra plongeante, personnage à la troisième personne, zones de taille limitée, à la manière de Minecraft Dungeons.
- Deux échelles : cubes macro (gameplay, simulation) et micro-cubes de 4×4×4 par cube macro, utilisés pour les particules quand un cube est récolté, perturbé ou détruit.
- Apparence entièrement fonction de l'état local : concentration, température, énergie, âge.
- Son procédural : la dynamique est sonifiée, un régime stable bourdonne, une oscillation rythme, l'approche du chaos devient dissonante.
- La lisibilité ne repose jamais sur la seule teinte : luminance, mouvement et son portent aussi l'information.

## Premier prototype

Une seule parcelle, deux réglages (flux et dissipation), la récolte comme perturbation, le ralentissement critique comme signal, l'hystérésis. Critère de réussite : pousser cette parcelle vers son rendement maximal sans la faire basculer est prenant pendant vingt minutes, même avec des graphismes bruts.

## Dimension recherche

Le jeu sert aussi de laboratoire : les régimes découverts (paramètres, conditions initiales, métriques) peuvent être enregistrés pour explorer l'espace des systèmes dissipatifs.

## À trancher

- Particules purement visuelles ou porteuses de matière (conservation).
- Difficulté : exigeant, avec effondrements possibles de la ferme, ou plus apaisé.
- Univers et ambiance : planète étrangère, monde microscopique, abstrait.
