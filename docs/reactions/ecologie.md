# L'écosystème : variables, flux, échelles, bascules

Code : `crates/game/src/ecology.rs` (plantes, pâturage, feu), `crates/game/src/soil.rs` (sol), `crates/game/src/deer.rs` (harde).

## Intuition

Un écosystème est une structure dissipative : il ne tient que tant qu'un flux le traverse (la pluie, la lumière). Il a deux vitesses :
- **rapide** (jours) : les plantes poussent, meurent, se dispersent ; les cerfs broutent, mettent bas, meurent de faim ;
- **lente** (semaines) : le sol garde l'eau et la matière organique que la végétation lui a données.

Les deux se tiennent l'un l'autre. Une prairie couverte retient l'eau qui la nourrit ; un sol nu la laisse ruisseler et reste nu. Il y a donc, sur une large plage de conditions, **deux états stables** pour la même pluie. Un écosystème qu'on pousse trop (surpâturage, brûlis répétés, défrichement) **bascule** dans l'autre état, et n'en revient pas quand la pression cesse : c'est l'**hystérésis**. Les écosystèmes réels le font (savane ↔ forêt, prairie ↔ désert, lac clair ↔ lac eutrophe).

## Variables

| Échelle | Variable | Où | Vitesse |
|---|---|---|---|
| plante | taille s_i ∈ (0, 1] | chaque plante | jours |
| parcelle 8 × 8 cases | couverture B (somme pondérée des tailles : herbe 1, arbuste 3, arbre 10, divisée par 50) | `Soil::cover` | suit les plantes |
| parcelle | eau du sol W | `Soil::water` | ~ 1 jour |
| parcelle | matière organique N | `Soil::humus` | ~ 30 jours |
| parcelle | pluie P (climat, proximité de l'eau) | fixe | — |
| harde | nombre de cerfs, réserves e_k ∈ [0, 1] de chacun | `Herd` | jours |

## Flux et équations

**Plantes** (Lotka-Volterra à plusieurs espèces, voir l'en-tête de `ecology.rs`) :

```text
ds_i/dt = r s_i (1 − (s_i + Σ α_ij w_ij s_j) / (H_i · S))
```

H_i est l'habitat (biome, humidité, lumière) et S ce que le sol offre, rapporté à l'état de départ du monde (le monde généré est à l'équilibre).

**Sol** (d'après Rietkerk et al. 2002) :

```text
infiltration   f(B) = (B + k w₀) / (B + k)          k = 0,15, w₀ = 0,1
dW/dt = P f(B) − e W − u B W + D ΔW                  e = 0,6, u = 0,4, D = 0,25 /jour
dN/dt = λ (N*(B) − N),  N*(B) = 0,3 + 0,7 min(B, 1)  λ = 0,03 /jour
S = W⁵ / (W⁵ + W_c⁵) · (0,2 + 0,8 N),  W_c = 0,5 P + 0,04
```

- f(B) est la **rétroaction positive** : sous un couvert, 100 % de la pluie s'infiltre ; sur sol nu, 10 %.
- La puissance 5 fait de W_c un **seuil** net.
- D ΔW est l'eau qui suinte d'une parcelle à ses voisines : un laplacien à 5 points, calculé par double tampon.
- Les cendres d'un feu rendent un peu de matière organique d'un coup.

**Pâturage** (réponse fonctionnelle de Holling de type II) : un cerf qui broute mange, autour de lui (1,5 case), au plus

```text
I = I_max · A / (A + A_½) · dt        I_max = 30 par jour de broutage, A_½ = 1
```

où A est le fourrage à portée, réparti selon l'appétence : herbe 1, fleurs 0,8, jeunes arbres 1,3 (c'est ce qui **garde la prairie ouverte**), fougères 0,05. Une plante broutée sous la taille de survie meurt.

**Harde** :

```text
de_k/dt = 0,2 · (ce que la biche a mangé) / taille − 1 · taille   (par jour)
naissances : 0,08 /jour par biche adulte bien nourrie (e > 0,75), × (1 − n/9)
```

Une biche à e ≤ 0 meurt. Une biche affamée broute aussi pendant ses heures de repos (alimentation compensatoire). La harde change de pâture quand la sienne a moins de 30 % du fourrage de la meilleure à 40 cases, et revient quand elle a repoussé (60 %).

## Régimes connus

- **Modèle de champ moyen d'une parcelle** (testé dans `soil.rs`) : sous P = 0,5, quand la pression de pâturage G monte, la couverture baisse doucement, puis s'effondre à G ≈ 0,7. Quand G redescend, elle ne revient pas, même à G = 0 : la parcelle seule est piégée dans l'état nu.
- **Bord ou cœur** : une tache nue au milieu d'une prairie reçoit l'eau de ses voisines et se referme par les bords (testé : 43 % de la couverture revenue en 15 jours). Une grande étendue mise à nu reste nue (testé : 4 %).
- **Harde et prairie** sur 40 jours sans le joueur : 7 à 8 cerfs, une prairie broutée mais vivante (couverture 0,5 à 0,7), un pâturage tournant entre trois pâtures. Sans la densité-dépendance des naissances, on observait un cycle d'emballement et d'effondrement (11 cerfs, puis 3) : ce sont les oscillations du modèle de Rosenzweig-MacArthur.

## Phénomènes réels à observer en jeu

- **Hystérésis** et **bascule** (« tipping point ») : un surpâturage concentré rend une prairie au désert.
- **Ralentissement critique** : près du seuil, une prairie met de plus en plus de temps à se remettre d'une perturbation. C'est un signal précoce de bascule, mesurable dans le jeu.
- **Rôle des herbivores** : sans les cerfs, les jeunes arbres envahissent la prairie (succession) ; avec trop de cerfs, elle s'épuise.

## Ce que le joueur y fait, sans le vouloir

- Effrayer la harde de ses autres pâtures la concentre sur une seule, qui peut basculer.
- Brûler répète des cendres (un regain) puis un sol nu.
- Prélever de l'herbe, creuser, défricher font baisser la couverture.

## Simplifications à garder en tête

- La pluie est le climat moyen : il n'y a pas d'années sèches ni d'orages qui ravinent.
- Pas de saisons ; les faons naissent toute l'année.
- Pas de prédateurs : la harde n'est régulée que par sa nourriture et sa densité.
- La couverture d'une parcelle est une somme pondérée simple, et la matière organique suit la couverture sans compter la litière morte espèce par espèce.
- Le sol des biomes très clairsemés (montagne, plage) est déjà presque à l'état nu au départ.

## Références

- M. Rietkerk et al., *Self-organization of vegetation in arid ecosystems*, The American Naturalist, 2002 : l'infiltration, les états alternatifs et les motifs de végétation.
- I. Noy-Meir, *Stability of grazing systems: an application of predator-prey graphs*, Journal of Ecology, 1975 : la bistabilité du pâturage.
- M. Scheffer et al., *Catastrophic shifts in ecosystems*, Nature, 2001 : hystérésis et signaux précoces.
