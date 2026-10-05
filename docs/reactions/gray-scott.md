# Gray-Scott

Première règle de réaction du projet. Fiche à compléter au fil des expériences.

## Équations

Deux espèces chimiques, U (substrat) et V (activateur autocatalytique) :

- ∂u/∂t = Du ∇²u − u v² + F (1 − u)
- ∂v/∂t = Dv ∇²v + u v² − (F + k) v

## Paramètres

| Paramètre | Sens physique | Sens dans le jeu |
|---|---|---|
| F | taux d'alimentation en U, retrait de tout | flux entrant |
| k | taux de disparition de V | dissipation |
| Du, Dv | coefficients de diffusion (Du ≈ 2 Dv) | fixes au départ |

## Stabilité numérique

Euler explicite, laplacien 7 points en 3D, dx = 1 : il faut D·dt ≤ 1/6 pour chaque espèce. Valeurs testées : Du = 0,16, Dv = 0,08, dt = 1.

## États stationnaires homogènes

L'état trivial (u, v) = (1, 0) existe toujours. Des états non triviaux existent quand (F + k)² < F / 4, c'est-à-dire k < √F / 2 − F. Cette courbe est une ligne de bifurcation selle-nœud : les motifs intéressants vivent près d'elle.

## Régimes observés (grille 48³, conditions périodiques, 8 germes initiaux, 6000 pas)

Mesure : pourcentage de cellules avec v > 0,18, relevé tous les 1000 pas.

| F | k | Comportement |
|---|---|---|
| 0,0367 | 0,0649 | cellules qui se divisent puis se stabilisent (11 % → 22 %) |
| 0,029 | 0,057 | labyrinthe qui remplit l'espace (~48 %, stable) |
| 0,030 | 0,062 | bulles stables (~26 %) |
| 0,046 | 0,063 | croissance lente et continue (3 % → 16 %) |
| 0,039 | 0,058 | structure dense stable (~59 %) |
| 0,022 | 0,051 | oscillations amples, croissance puis effondrement partiel |
| 0,018 | 0,051 | régime erratique, booms et effondrements répétés |
| 0,026 | 0,051 | oscillation globale puis extinction totale |
| 0,020 | 0,070 | extinction immédiate |
| 0,0545 | 0,062 | germes figés, aucune croissance (~1 %) |

Ces résultats viennent d'un test rapide et d'une seule graine : à confirmer avec la skill `balayage-parametres`.
