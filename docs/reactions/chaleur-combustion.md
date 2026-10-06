# Chaleur et combustion (réseau thermique)

Modèle de `crates/sim/src/thermal.rs`. Il anime les objets posés dans le monde : un feu, ce sont des brindilles qui brûlent ; un foyer, des pierres qui chauffent autour ; une coupelle cuit parce qu'elle est restée chaude assez longtemps. Aucune recette : la physique décide.

## Intuition

Chaque objet est un **corps** à température uniforme. Les corps échangent de la chaleur deux à deux (contact, rayonnement), en perdent vers l'air, et certains brûlent. Un corps qui brûle chauffe ses voisins jusqu'à les allumer : le feu est un **milieu excitable**, comme la réaction de Belooussov-Jabotinski (seuil, flambée, épuisement du combustible).

## Équation

```text
C_i dT_i/dt = Σ_j [ G_ij (T_j − T_i) + k_ij (T_j⁴ − T_i⁴) ]     échanges entre corps
            + panache des voisins qui brûlent                     gaz chauds de la flamme
            − h A_i (1 − e_i)(T_i − T_air)                        convection vers l'air
            − ε σ A_i (1 − e_i)(T_i⁴ − T_air⁴)                    rayonnement vers le ciel
            + r P_i                                               combustion (part retenue r)
```

- **C_i = m c** : capacité thermique ; seule la **couche fine** du combustible compte (« part chauffée »).
- **Σ_j G_ij (T_j − T_i)** : un **laplacien sur un graphe**, la même diffusion que le laplacien 7 points de Gray-Scott, entre objets reliés au lieu de cellules voisines.
- **k_ij = ε σ A F** : rayonnement, avec un facteur de vue F ≈ r_j²/d² (borné). Il est calculé **exactement** en T⁴ : le linéariser autour de 600 K le sous-estimerait d'un facteur 5 près d'un feu à 1200 K.
- **e_i, l'enfermement** : la somme des angles solides sous lesquels un corps voit ses voisins, Ω/4π = (1 − cos θ)/2 avec sin θ = r/d. Un corps enfermé perd moins de chaleur… mais reçoit moins d'air pour brûler : un four doit garder une ouverture.
- **Panache** : un corps qui brûle donne 40 % de sa puissance aux corps qui le touchent, répartis selon leur surface, et seulement s'ils sont plus froids que les gaz de la flamme (~1300 K) : c'est un transfert convectif.

## Combustion

Un corps combustible brûle s'il dépasse sa température d'allumage, s'il est sec et si l'air lui arrive. Sa vitesse croît sur 150 K au-delà du seuil (une rampe à la place de la loi d'Arrhenius). Il dégage P = ṁ × pouvoir calorifique (16 MJ/kg pour le bois sec) ; il en garde une part r :

| Combustion | Part retenue r | Pourquoi |
|---|---|---|
| Flamme (herbe, brindilles) | 5 % | la chaleur part avec les gaz ; l'équilibre avec le rayonnement donne ~1100-1250 K |
| Braise (sans flamme) | 60 % | la réaction a lieu à la surface du charbon, qui garde sa chaleur |

**Allumage piloté** : des brindilles fines prennent vers 270 °C quand une flamme est là pour enflammer leurs gaz ; sans flamme, le bois demande plus de 300 °C.

## Eau

- **Palier d'ébullition** : tant qu'un corps contient de l'eau, il ne dépasse pas 100 °C ; l'énergie en trop la vaporise (L = 2,26 MJ/kg). C'est pourquoi le bois humide s'allume si mal, et pourquoi une coupelle encore humide **éclate** dans le feu (la vapeur la fait sauter).
- **Séchage lent** sous 100 °C : **accéléré pour le jeu** (une argile sèche en une demi-heure à l'air, quelques minutes près d'un feu) ; sa chaleur latente est négligée.

## Ce que la physique enseigne sans tutoriel

- Une braise seule n'allume pas un fagot : il faut de l'**amadou** (herbe sèche).
- Des brindilles posées d'emblée sur une seule touffe lui prennent sa chaleur et son air : elle ne prend pas. **Amadou d'abord, petit bois ensuite**, et un nid de deux poignées plutôt qu'une.
- Une coupelle doit **sécher** avant le feu ; cuite au-dessus de 600 °C pendant une minute, elle devient terre cuite (un feu de brindilles l'amène vers 840 °C, la plage réelle de la terre cuite).

## Intégration

Euler explicite, pas découpé automatiquement : dt ≤ ½ min C_i / G_i, où G_i est la conductance totale du corps (rayonnement linéarisé à 1300 K, le pire cas). Déterministe.

## Simplifications à garder en tête

- Température uniforme par objet (modèle à constantes localisées), compensée pour le combustible par la « part chauffée ».
- Pas de vrai écoulement d'air : l'air arrive selon l'enfermement ; pas encore de tirage (cheminée).
- Panache, enfermement et facteurs de vue sont des approximations géométriques.
- Séchage accéléré.

## Mesures de calibration

| Scène | Résultat |
|---|---|
| Braise dans un nid de 2 poignées d'herbe | l'herbe s'enflamme vers 60 s |
| Brindilles posées sur l'herbe en flamme | elles prennent en ~30 s, brûlent ~1 min à ~1100 K, ~45 kW |
| Coupelle sèche entourée de 6 fagots | ~1110 K (840 °C), cuite en ~70 s |
| Pierre à 20 cm d'un fagot qui brûle | +50 K en 20 s |
