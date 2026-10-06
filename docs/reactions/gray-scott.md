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

Remarque : les paramètres de référence (F = 0,0367, k = 0,0649) sont **au-delà** de cette ligne ((F + k)² ≈ 0,0103 > F/4 ≈ 0,0092). Aucun état homogène de coexistence n'existe, et pourtant les cellules survivent : les motifs ne sont pas des perturbations d'un état uniforme, ce sont des structures localisées qui se maintiennent seules.

## Taille critique des germes

Un germe cubique (u, v) = (0,5, 0,25) doit être assez gros pour prendre (F = 0,0367, k = 0,0649, 6000 pas, `reset_with_seeds`) :

| Germes | Taille | Résultat |
|---|---|---|
| 4 dans 24³ | 3³, 4³ | extinction avant 1000 pas |
| 4 dans 24³ | 5³ | croissance, ~22 % de cellules avec v > 0,18 |
| 8 dans 48³ | 3³, 4³ | extinction |
| 8 dans 48³ | 5³ | croissance, ~22 % |

C'est une nucléation : un petit germe perd plus de V par diffusion à sa surface qu'il n'en produit en volume. Pour le jeu, une semence trop petite meurt ; c'est un seuil qu'on peut rendre lisible.

Dans un bassin à parois étanches (`Boundary::NoFlux`, 40 × h × 40), tout prend, même avec des germes réduits à la hauteur du bassin : la paroi renvoie le V au lieu de le laisser fuir.

| Bassin | Germes | Cellules actives après 1000 → 6000 pas |
|---|---|---|
| 40×4×40 | 8 de 5³ (réduits à 4 en hauteur) | 21 % → 23 % |
| 40×6×40 | 8 de 5³ | 22 % → 23 % |
| 40×8×40 | 8 de 5³ | 20 % → 24 % |

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

## En 2D : le tapis (une seule couche, ny = 1)

Avec `Boundary::NoFlux` et ny = 1, les voisines en y sont la cellule elle-même : le laplacien 7 points devient le laplacien 5 points du plan. Mesures sur 48 × 1 × 48, 6 germes, pourcentage de cellules avec v > 0,18 tous les 1000 pas :

| F | k | 1000 → 6000 pas | Allure |
|---|---|---|---|
| 0,0367 | 0,0649 | 15 % → 24 % | cellules qui se divisent |
| 0,029 | 0,057 | 45 % → 46 % | labyrinthe, stable |
| 0,030 | 0,062 | 21 % → 25 % | bulles |
| 0,046 | 0,063 | 32 % → 44 % | croissance continue |
| 0,039 | 0,058 | 83 % → 80 % | presque tout envahi |
| 0,022 | 0,051 | 46 % → 54 %, irrégulier | agitation, ne se fige pas |
| 0,025 | 0,060 | 18 % → 27 %, irrégulier | idem |

Contrairement à la 3D, la taille des germes (5, 8 ou 12) change peu le résultat sur cette grille : tout prend. Les deux dernières lignes oscillent d'une mesure à l'autre : ces régimes ne se figent pas, ils restent agités.
