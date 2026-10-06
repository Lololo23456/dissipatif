# Oregonator (Belooussov-Jabotinski)

Modèle de la réaction de Belooussov-Jabotinski (BZ), réduit à deux variables par Tyson et Fife. Il anime les récipients du laboratoire : oscillations en solution agitée, ondes et spirales en couche mince.

## Intuition

La réaction a deux acteurs :
- un **activateur** u (l'acide bromeux, HBrO₂) qui se **fabrique lui-même** : plus il y en a, plus il en apparaît (autocatalyse). C'est l'étincelle ;
- un **inhibiteur** v (le catalyseur oxydé, Ce⁴⁺ ou Mn³⁺) qui suit u avec retard et, par le bromure qu'il libère, **éteint** l'autocatalyse.

Un point au repos est **excitable** : une petite perturbation s'efface, mais une perturbation au-delà d'un **seuil** déclenche une flambée d'activateur, suivie d'une longue période **réfractaire** pendant laquelle l'inhibiteur empêche toute nouvelle flambée. Avec la diffusion, la flambée excite ses voisines : une **onde** se propage, et derrière elle la zone réfractaire l'empêche de revenir en arrière. Une onde dont l'extrémité est libre s'enroule autour d'elle-même : c'est une **spirale**. Le même mécanisme fait battre le cœur (ondes électriques) et s'agréger les amibes *Dictyostelium*.

## Équations (Tyson-Fife, sans dimension)

```text
∂u/∂t = (1/ε) [ u (1 − u) − f v (u − q) / (u + q) ] + Du ∇²u
∂v/∂t = u − v                                        + Dv ∇²v
```

| Symbole | Sens | Valeurs usuelles |
|---|---|---|
| u | activateur (HBrO₂, mis à l'échelle) | 0 au repos, ~1 dans l'onde |
| v | inhibiteur, la forme oxydée du catalyseur (Ce⁴⁺ jaune, Mn³⁺ rouge) : **c'est lui qu'on voit** | 0 à ~0,3 |
| ε | rapport des temps : u est beaucoup plus rapide que v | 0,01 à 0,1 |
| q | constante de vitesse réduite | ~0,002 |
| f | facteur stœchiométrique : bromure libéré par catalyseur réduit | 0,5 à 2,4 (excitable vers 1 à 1,4) |
| Du, Dv | diffusions. Dv = 0 si le catalyseur est fixé dans un gel | Du = 1, Dv = 0 à 0,6 |

L'état (u, v) = (0, 0) est un repos stationnaire (le terme de réaction s'annule).

## Raideur : la vraie difficulté numérique

Le système est **raide** : u varie sur un temps ~ε, v sur un temps ~1. Pire, près de u ≈ q le terme f v (u − q)/(u + q) varie très vite : sa dérivée par rapport à u vaut f v · 2q / (u + q)², soit environ f v / (2q) en u = q. Avec f = 1,4, v = 0,1, q = 0,002, ε = 0,02 : (1/ε) · 35 ≈ 1750. Euler explicite demande alors dt ≲ 1 / 1750, quelques 10⁻⁴.

Conditions à respecter en Euler explicite :
- **réaction** : dt ≲ ε · 2q / (f v_max) environ (à mesurer ; c'est la contrainte dominante) ;
- **diffusion** (pas d'espace dx) : Du · dt / dx² ≤ 1/4 en 2D (1/6 en 3D).

Pistes pour aller plus vite, une fois la version simple écrite et testée :
- **séparer** réaction et diffusion (*operator splitting*) et traiter la réaction de u de façon implicite (une équation scalaire par cellule) ;
- prendre ε un peu plus grand (0,05) : moins raide, motifs plus gros et plus lents.

## Ce que le jeu en fait

- **Agité** (sans diffusion) : toute la solution oscille, jaune ↔ incolore.
- **Couche mince** (boîte de Petri) : cibles et spirales.
- **Température** : multiplie toutes les vitesses (loi d'Arrhenius) ; dans le modèle, un facteur sur dt.
- **Bromure en excès** (saumure mal purifiée) : il inhibe ; dans le modèle, il revient à augmenter f ou à décaler le seuil (à calibrer).

## Sources

- [Tyson-Fife reduction, two-variable Oregonator (Physica A, 2021)](https://par.nsf.gov/servlets/purl/10218179)
- [Reduction waves in the two-variable Oregonator model for the BZ reaction](https://www.sciencedirect.com/science/article/abs/pii/S0167278912001182)
- [Travelling waves in the Oregonator model for the BZ reaction](https://www.researchgate.net/publication/237675811_Travelling_waves_in_the_Oregonator_model_for_the_BZ_reaction)
