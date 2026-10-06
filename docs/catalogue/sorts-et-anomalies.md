# Sorts et anomalies

## Comment fonctionne une anomalie

Une anomalie est un comportement étrange d'êtres vivants (ou d'un milieu). Elle a toujours :

| Élément | Exemple (le rituel des cerfs) |
|---|---|
| **Des conditions de vie** : elle naît là où un milieu est sain depuis longtemps, et meurt quand il se dégrade | une harde qui prospère dans une prairie ouverte |
| **Un lieu** | le cercle de la prairie |
| **Un moment** : heure, lune, saison | les nuits de pleine lune, vers 22 h |
| **Des présages** : ce qui arrive avant | la meneuse qui tourne seule les nuits de lune gibbeuse |
| **Des indices** : ce qui reste après | l'anneau de terre foulée, les traces de sabots toutes dans le même sens |
| **Une épreuve d'observation** | se placer contre le vent, accroupi, sans feu, et observer jusqu'au sommet du rituel |
| **Ce qu'elle apprend** | la Forme du cerf |

Toutes battent au rythme du **grand cycle de la source** : elles sont plus fortes à certaines périodes, et un joueur attentif finira par voir qu'elles partagent une même période.

Elles s'apparentent à des **structures dissipatives** réelles : synchronisations (lucioles, chœurs, rituels), fronts (cercles de champignons), spirales (réactions oscillantes), motifs de Turing. `crates/sim` les anime.

## Comment fonctionne un sort

- On le lance avec une **roue des sorts** : on maintient la touche, une roue s'ouvre avec les sorts connus, on vise et on relâche. Les sorts y sont dessinés (les signes du carnet), pas écrits.
- Il agit par la **vraie physique** du monde (feu, eau, croissance, lumière), et toujours avec un **coût ailleurs** : une pluie appelée assèche une autre vallée, une croissance forcée épuise le sol.
- Les **formes** (cerf, chouette, nuée…) sont des sorts qui durent, sans les mains.
- **Chaque forme apprise efface une part des mots du carnet**, changés en signes. En partie réversible : rester humain longtemps rend des mots.

## L'arbre caché

Un sort ouvre une manière de percevoir qui permet d'observer l'anomalie suivante. Le joueur ne voit jamais l'arbre ; il le découvre.

```text
Rituel des cerfs ──► FORME DU CERF ✅
                         │  approcher la faune sans la faire fuir, marcher sans bruit
                         ▼
Cercle des chouettes ──► ŒIL DE CHOUETTE
                         │  voir et entendre la nuit
              ┌──────────┴───────────┐
              ▼                      ▼
Chœur des lucioles          Tourbillon du lac (nouvelle lune)
   ──► CHŒUR DES LUCIOLES       ──► SOUFFLE DES EAUX
              │                      │  le fond des lacs et des marais
              ▼                      ▼
Spirale du marais          Cercle des fées
   ──► FEU FOLLET              ──► CERCLE DES FÉES
              └──────────┬───────────┘
                         ▼
Murmuration d'étourneaux ──► FORME DE LA NUÉE
                         │  survoler la planète
                         ▼
Corbeaux de l'orage ──► APPEL DE L'ORAGE
Renard qui saute vers le nord ──► SENS DU NORD ──► la source, la grande marée
```

L'arbre n'est pas figé : plusieurs chemins peuvent mener au même sort, pour qu'un joueur ne reste jamais bloqué sur une seule branche.

## Les sorts

| Sort | Appris de | Effet | Coût, risque | Ouvre |
|---|---|---|---|---|
| **Forme du cerf** ✅ | le rituel des cerfs | devenir cerf (mâle, avec ses bois) : la harde vous accepte, vitesse et bonds, pas de mains | des mots s'effacent | approcher la faune |
| **Œil de chouette** | le cercle des chouettes | voir dans le noir, entendre ce qui bouge, vol silencieux | des mots s'effacent ; ébloui par le feu | les anomalies nocturnes |
| **Chœur des lucioles** | les lucioles synchronisées | une lumière qu'on appelle et qu'on envoie ; guide, éclaire, attire les papillons de nuit | attire aussi les prédateurs | les sous-bois profonds |
| **Souffle des eaux** | le tourbillon du lac | respirer et marcher sous l'eau | le froid de l'eau (chaleur) | le fond des lacs et des marais |
| **Feu follet** | la spirale du marais | une flamme froide qui dérive avec le vent et allume vraiment ce qu'elle touche | l'incendie (la prairie sèche brûle vraiment) | la lande |
| **Cercle des fées** | l'anneau de champignons | un front vivant qui fait pousser à son passage | épuise le sol derrière lui (humus) | la terre à soigner |
| **Forme de la nuée** | la murmuration | devenir un oiseau parmi d'autres, voler | des mots s'effacent ; fatigue | la planète vue d'en haut |
| **Appel de l'orage** | les corbeaux de l'orage | la foudre tombe où l'on veut | fend un arbre, allume un incendie ; les corbeaux s'en souviennent | les sommets |
| **Sens du nord** | le renard du nord | sentir l'onde de la source ; s'orienter ; lire la date de la grande marée | — | la source |

## Les anomalies

| Anomalie | Êtres | Lieu | Moment | Présages, indices | Phénomène réel derrière |
|---|---|---|---|---|---|
| Le cercle des cerfs ✅ | cerfs | prairie ouverte | pleine lune, 22 h | meneuse seule ; anneau de terre | — |
| Le cercle des chouettes | chouettes | arbre mort en lisière | nuits sans lune | pelotes de réjection en anneau au pied de l'arbre | — |
| Le chœur des lucioles | lucioles | vieille forêt humide | nuits chaudes d'été | clignotements qui se rapprochent nuit après nuit | synchronisation (Kuramoto) |
| Le tourbillon du lac | poissons | fond d'un lac | nouvelle lune | sable dessiné en spirale au fond | — |
| La spirale du marais | (le marais lui-même) | marais riche, eau calme | été | algues en anneaux concentriques | réaction de Belooussov-Jabotinski |
| Le cercle des fées | champignons | prairie ou lisière | automne, après la pluie | herbe plus verte en anneau | front de mycélium |
| La murmuration | étourneaux | au-dessus d'un marais | crépuscules d'hiver | toujours la même forme | murmurations réelles |
| Les corbeaux de l'orage | corbeaux | arbre foudroyé | avant l'orage | arbre noirci, plumes | funérailles des corvidés |
| Le renard du nord | renard | prairie enneigée | hiver | traces de bonds toutes orientées pareil | bond magnétique des renards |

## Questions ouvertes

- Combien de sorts au total ? Une dizaine bien reliés plutôt que des dizaines.
- Peut-on oublier un sort ? (Une anomalie morte enlève-t-elle le sort appris ? Probablement non, mais elle empêche de le transmettre.)
- ~~Le choix du sort à lancer~~ : décidé, une roue.
