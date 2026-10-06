# Plantes et milieux

Les plantes vivent (Lotka-Volterra à plusieurs espèces, ombre, sol, feu, pâturage : `docs/reactions/ecologie.md`). Chaque milieu a ses espèces, ses saisons, et **ses états alternatifs** : ce vers quoi il bascule quand on le pousse trop.

Statut : ✅ en jeu · ⬜ à faire.

## Plantes

| Plante | Milieu | Vie | Saisons (à faire) | Statut |
|---|---|---|---|---|
| Herbe | prairie, savane | pionnière, rapide, brûle bien sèche | verte au printemps, jaunie l'été, rase l'hiver | ✅ |
| Fleurs sauvages (pâquerette, coquelicot, lavande, bouton-d'or) | prairie | vie courte, graines portées loin | floraison au printemps et en été | ✅ |
| Fougère | sous-bois | lente, ombre humide, amère (non broutée) | rousse et sèche l'hiver | ✅ |
| Champignons | sous-bois humide | fructifications brèves | surtout l'automne après la pluie | ✅ |
| Buisson | lisière | lent, longue vie | baies l'été (à faire) | ✅ |
| Arbuste sec | savane, désert | rustique, amadou | — | ✅ |
| Feuillu | forêt | pousse à l'ombre, très longue vie | feuilles qui jaunissent et tombent | ✅ |
| Bouleau | forêt | pionnier, aime la lumière | jaune d'or à l'automne ; écorce | ✅ |
| Pin | taïga | lent, résineux | toujours vert | ✅ |
| Saule | bord de l'eau | aime l'humidité | chatons au printemps | ✅ |
| Acacia, palmier, cactus | savane, plage, désert | climats secs | — | ✅ |
| Chêne | forêt | glands, années de forte production | glands à l'automne | ⬜ |
| Noisetier | lisière | noisettes | chatons à la fin de l'hiver | ⬜ |
| Ronces, sureau, myrtilles | lisière, sous-bois | baies | été, automne | ⬜ |
| Roseaux, joncs | marais | vannerie, toit | dorés l'hiver | ⬜ |
| Mousses, lichens | rochers, troncs | très lents ; les lichens indiquent un air pur | — | ⬜ |
| Algues, lentilles d'eau | marais, lacs | rapides ; trop nombreuses : l'eau étouffe | prolifèrent l'été | ⬜ |
| Mycélium (cercle des fées) | prairie, lisière | front qui s'étend d'année en année | — | ⬜ |

## Milieux et leurs bascules

| Milieu | Ce qui le tient | Vers quoi il bascule | Ce qui le pousse | Statut |
|---|---|---|---|---|
| **Prairie** | l'eau qui s'infiltre sous l'herbe ; les cerfs qui broutent les jeunes arbres | **sol nu** (trop pâturée, brûlée, défrichée) ; **forêt** (sans herbivores) | surpâturage, feux répétés ; ou l'absence de cerfs | ✅ (sol nu) / ⬜ (forêt) |
| **Forêt** | l'ombre, l'humus, l'humidité | **lande** (après des incendies répétés) | feu, coupes | ⬜ |
| **Savane** | un équilibre entre herbe, arbres épars et feux | **forêt sèche** (sans feux), **sol nu** (surpâturage) | le régime des feux | ⬜ |
| **Marais** | l'eau stagnante, les castors | **prairie humide** (drainé), **eau verte** (trop riche : les algues étouffent) | barrage détruit, cendres et terre lessivées dans l'eau | ⬜ |
| **Lac** | une eau claire, des plantes au fond | **eau trouble** (eutrophisation : un vrai cas d'hystérésis) | trop de matière apportée | ⬜ |
| **Désert** | la sécheresse | rarement autre chose : un désert né d'une prairie y reste | — | ✅ |
| **Montagne** | le froid, le sol mince | **éboulis nus** | érosion | ⬜ |

## Saisons (étape 1)

| Saison | Plantes | Bêtes | Monde |
|---|---|---|---|
| Printemps | floraison, jeunes pousses, chatons | faons, nids, chœurs de grenouilles | pluies, crues |
| Été | herbe qui sèche, baies | lucioles, abeilles | orages, incendies possibles |
| Automne | feuilles qui tombent, champignons, glands | brame des cerfs, murmurations | brumes, premières gelées |
| Hiver | arbres nus, herbe rase | bois de cerf qui tombent, renards qui mulotent dans la neige | neige, lacs gelés |

À trancher : la durée d'une saison. Deux lunes (16 jours de jeu, environ 5 h de jeu réel) donneraient une année d'environ 21 h de jeu : assez long pour que chaque saison compte, assez court pour en voir plusieurs.
