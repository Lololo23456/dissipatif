---
paths:
  - "crates/sim/**"
  - "crates/render/shaders/sim_*.wgsl"
---
# Règles pour le cœur de la simulation

## Mode pédagogique (le plus important)
L'utilisateur veut comprendre et écrire lui-même le cœur numérique : équations, laplacien, intégrateurs, règles de réaction.
- Avant toute implémentation numérique : explique l'idée (intuition, équations, condition de stabilité), puis propose une structure ou du pseudo-code, et laisse l'utilisateur écrire.
- Ensuite relis son code : bugs, stabilité, performance. Pose des questions qui l'aident à trouver lui-même les erreurs avant de les donner.
- Tu écris l'implémentation directement seulement si l'utilisateur le demande explicitement (« écris-le », « fais-le »).
- Tu peux toujours écrire toi-même : tests, benchmarks, outillage de mesure, structures de données sans contenu numérique.

## Exigences numériques
- Vérifie la condition de stabilité de chaque schéma explicite. Euler + laplacien 7 points en 3D : D·dt ≤ 1/6. Les termes de réaction ajoutent leurs propres contraintes : estime-les.
- Double tampon : on lit l'état n, on écrit l'état n+1, on échange. Jamais de mise à jour en place.
- Conditions aux bords explicites et documentées (périodiques, Neumann, Dirichlet). Pas de bord implicite par accident.
- Les paramètres physiques ont des noms qui disent leur sens (`feed_rate`, `kill_rate`, `diffusion_u`), pas `a`, `b`, `c`.
- Aucune aléa sans graine explicite. Même graine → même résultat sur CPU.

## Performance
- Pas d'allocation dans la boucle de pas de temps.
- Parcours mémoire contigu : x varie le plus vite. Précalcule les indices des voisins périodiques plutôt qu'un modulo dans la boucle interne.
- Repère : un pas Gray-Scott sur 48³ prend environ 1 ms sur CPU en code simple. Beaucoup plus lent signale un problème.

## Tests attendus pour toute nouvelle règle
- Un état stationnaire homogène reste stationnaire.
- Déterminisme : deux exécutions à même graine sont identiques bit à bit.
- Pas d'explosion (valeurs finies et bornées) sur un long run aux paramètres de référence.
- Si une quantité doit être conservée, un test le vérifie.
- La diffusion seule, partie d'une impulsion, s'étale de façon symétrique.

## Version GPU
Écrite seulement après la version CPU testée. Validée contre elle sur une petite grille, avec tolérance flottante, jamais à l'égalité exacte.
