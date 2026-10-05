---
name: balayage-parametres
description: Explore automatiquement l'espace des paramètres d'une règle de réaction en lançant de nombreuses simulations sans affichage, mesure leur comportement et classe les régimes (extinction, stable, motifs, oscillant, chaotique). À utiliser dès que l'utilisateur veut trouver des régimes intéressants, cartographier des paramètres, tracer une carte des régimes ou un diagramme de bifurcation, chercher où une parcelle bascule, ou remplir le tableau des régimes d'une fiche de réaction.
argument-hint: [règle] [plage-param1] [plage-param2]
allowed-tools: Bash(cargo run --release -p lab *) Bash(cargo build --release -p lab *)
---

# Balayage de paramètres : $ARGUMENTS

Objectif : utiliser le calcul pour découvrir ce que les équations savent faire, puis en rendre compte à l'utilisateur.

## 1. Préparer
- Lis la fiche `docs/reactions/<règle>.md` : paramètres, plages plausibles, régimes déjà connus.
- Si `crates/lab` n'a pas encore de sous-commande `sweep`, propose de l'écrire (c'est de l'outillage : tu peux l'écrire toi-même). Interface conseillée :
  `cargo run --release -p lab -- sweep --rule <règle> --p1 <nom>:<min>:<max>:<n> --p2 <nom>:<min>:<max>:<n> --size 32 --steps 4000 --seeds 3 --out crates/lab/results/<date>-<règle>.csv`
- Commence petit : grille 32³, grille de paramètres 12×12, 3 graines. Estime le temps total avant de lancer (repère : ~1 ms par pas sur 48³) et préviens l'utilisateur si ça dépasse quelques minutes.

## 2. Mesurer
Pour chaque simulation, relève à intervalles réguliers les métriques décrites dans `references/metriques.md`. Lis ce fichier avant d'écrire ou modifier le code de mesure.

## 3. Classer
Applique les règles de classement de `references/metriques.md` pour attribuer un régime à chaque point. Garde les seuils dans le code, pas en dur dans plusieurs endroits, et indique-les dans le rapport.

## 4. Rapporter
Dans la conversation, donne :
- une carte des régimes en texte (grille de lettres, une par régime) avec la légende ;
- les frontières remarquables, en particulier les zones où un petit changement de paramètre fait changer de régime : ce sont les plus intéressantes pour le gameplay ;
- les points qui varient selon la graine (multistabilité, hystérésis possible) ;
- trois à cinq points de paramètres à essayer visuellement dans le jeu.

## 5. Garder une trace
- Le CSV reste dans `crates/lab/results/` (ignoré par git).
- Mets à jour le tableau des régimes dans `docs/reactions/<règle>.md` avec les résultats confirmés, en précisant taille de grille, nombre de pas et nombre de graines.

## Hystérésis (variante)
Si l'utilisateur veut tester l'hystérésis : fais varier un paramètre lentement vers le haut puis vers le bas, sans réinitialiser l'état, et compare les métriques à l'aller et au retour. Un écart entre les deux courbes montre une hystérésis.
