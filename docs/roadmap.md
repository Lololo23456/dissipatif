# Feuille de route

Ordre de travail vers la vision (`vision.md`). Chaque étape a un objectif, un contenu et un critère de fin : « c'est fini quand… ». On ne passe à la suivante qu'une fois la précédente jouable. Les contenus détaillés sont dans `catalogue/`.

Légende : ✅ fait · 🔨 en cours · ⬜ à faire.

## Étape 0 — Le socle ✅

Un monde procédural vivant, et un naturaliste qui y vit.

- ✅ Monde voxel : relief, climat, 9 biomes, lacs et rivières, micro-voxels (creuser).
- ✅ Ambiance : jour et nuit, ciel étoilé, lune et ses phases, météo, vent qui tourne, son synthétisé, post-traitement.
- ✅ Le naturaliste : marcher, courir, nager, s'accroupir, gestes animés, traces.
- ✅ Survie douce : faim, soif, chaleur.
- ✅ Artisanat par la physique : chaleur, combustion, feu par friction, argile et cuisson, taille de la pierre, couteau.
- ✅ Écologie à deux vitesses : plantes (Lotka-Volterra), sol (eau, humus), bascules et hystérésis, incendies.
- ✅ La harde de cerfs : comportement réel, sens, pâturage, naissances et morts, rituel de pleine lune.
- ✅ Le carnet qui se remplit de croquis ; la Forme du cerf.

## Étape 1 — Une planète où l'on vit ⬜

Objectif : qu'on puisse **vivre** longtemps au même endroit.

- ⬜ **Sauvegarde** : le monde (creusé, brûlé, poussé), les plantes, le sol, la harde, le carnet, le joueur. Indispensable avant tout le reste.
- ⬜ **Saisons et années** : calendrier (4 saisons), température, neige et gel des lacs l'hiver, couleurs de la végétation, plantes qui fleurissent, fructifient et dorment.
- ⬜ **Écureuils** : caches de glands à l'automne, recherche l'hiver, caches oubliées qui germent en chênes ; ajout du chêne et du noisetier. Premier animal qui plante.
- ⬜ **Chaque animal vit au rythme des saisons**, d'après son calendrier réel (voir le tableau dans `catalogue/animaux.md`) : reproduction à sa saison seulement, mues, réserves, repos d'hiver.
  - Cerfs : le rut et le brame à l'automne, les naissances à la fin du printemps (environ huit mois de gestation), les bois qui tombent à la fin de l'hiver (un objet à ramasser), une harde qui s'amaigrit l'hiver. Remplace les naissances « toute l'année » actuelles.
  - Oiseaux : nids et chant au printemps, envols des jeunes en été.
  - Lucioles et papillons : seulement aux beaux jours ; chenilles et chrysalides avant.
  - Poissons : remontée des rivières pour frayer.
  - Écureuils : caches à l'automne, recherche l'hiver, petits au printemps.
- ⬜ **Le sol se voit** : un sol épuisé change de couleur, l'anneau du rituel s'efface s'il n'est plus foulé.
- ⬜ **La planète se referme** : bords du monde raccordés (on fait le tour), carte plus grande.
- ⬜ **Sons de la harde** : aboiement d'alarme, coup de sabot, brame.

C'est fini quand : on peut quitter le jeu, revenir, et retrouver le monde tel qu'on l'a laissé, une saison plus tard.

## Étape 2 — Le carnet qui perd ses mots ⬜

Objectif : la transformation a un coût visible et choisi.

- ⬜ Les métadonnées d'une page en **signes** : la lune dessinée, le vent en flèche, l'heure par le soleil, le lieu par une petite carte.
- ⬜ À chaque forme apprise, une part des mots du carnet se change en signes (lettres qui se défont, griffonnages).
- ⬜ Réversibilité partielle : rester humain longtemps sans prendre de forme rend des mots.
- ⬜ Le carnet devient l'outil d'hypothèses : relier des pages, marquer « même lieu », « même lune ».

C'est fini quand : un joueur qui a pris beaucoup de formes ne lit plus ses mots, mais mène encore son enquête avec les signes.

## Étape 3 — Des anomalies qui naissent de la vie ⬜

Objectif : la magie apparaît et disparaît avec l'état du monde, et ne s'épuise jamais.

- ⬜ **Un cadre générique** : une anomalie a ses conditions (un milieu sain depuis longtemps, une espèce assez nombreuse), sa période (lune, saison, heure), ses présages et ses indices. Elle naît quand ses conditions sont réunies, et meurt quand elles se dégradent.
- ⬜ **La spirale du marais** : une réaction-diffusion (Oregonator) dans l'eau d'un marais riche, animée par `crates/sim`. **Exercice pour l'utilisateur** : le cœur numérique, avec la règle `simulation.md`.
- ⬜ **Le chœur des lucioles** : des lucioles qui finissent par clignoter ensemble (oscillateurs couplés, modèle de Kuramoto).
- ⬜ **Le cercle des fées** : un anneau de champignons qui s'élargit (un front de mycélium).
- ⬜ Le rituel des cerfs passe dans ce cadre : il naît là où une harde prospère depuis longtemps.

C'est fini quand : soigner un lieu fait, un jour, naître une anomalie qu'on n'avait jamais vue.

## Étape 4 — L'arbre des sorts ⬜

Objectif : chaque sort ouvre une manière de percevoir qui mène au suivant.

- ⬜ La **roue des sorts** (maintenir, viser, relâcher).
- ⬜ Deuxième sort, débloqué grâce à la Forme du cerf.
- ⬜ Trois ou quatre sorts et formes reliés (voir `catalogue/sorts-et-anomalies.md`).
- ⬜ Chaque sort agit par la vraie physique (feu, eau, croissance), avec un coût ailleurs.

C'est fini quand : un joueur découvre une anomalie qu'il n'aurait pas pu voir sans un sort précédent, et comprend pourquoi.

## Étape 5 — Le refuge ⬜

Objectif : un lieu à soi, bâti avec ce que la nature donne.

- ⬜ Construction par la physique : clayonnage, torchis, chaume, pierre sèche (voir `catalogue/objets.md`).
- ⬜ Foyer, lit de fougères, séchoir, rangements.
- ⬜ Le bosquet autour : planter, guider une succession.

C'est fini quand : on a envie de rentrer chez soi le soir.

## Étape 6 — Une faune plus riche ⬜

Objectif : un écosystème complet, avec des prédateurs et des chaînes de causes.

- ⬜ Les espèces prioritaires de `catalogue/animaux.md` : chouette, renard, campagnols, étourneaux, castor.
- ⬜ Chaque animal agit sur la terre (voir le principe dans `catalogue/animaux.md`) : il transporte, retire, ajoute, transforme.
- ⬜ Chaînes trophiques : prédateurs qui régulent, cascades (le castor crée un marais, le marais une anomalie).

C'est fini quand : une cause lointaine (chasser les loups, abattre un barrage de castor) change un lieu qui n'a rien à voir en apparence.

## Étape 7 — La source ⬜

Objectif : le mystère central, et un rendez-vous qui revient.

- ⬜ L'onde lente qui parcourt la planète ; les anomalies battent à son rythme.
- ⬜ La grande marée : ce qu'elle fait (transformer, réparer, emporter), plus forte quand la planète est saine.
- ⬜ Le vieil homme de la forêt et ses rumeurs.

C'est fini quand : un joueur prédit la grande marée à partir de son carnet, et s'y rend.

## Plus tard

- Multijoueur (l'architecture le permet déjà : commandes, pas fixes).
- D'autres planètes, chacune régie par d'autres phénomènes.
- Des mystères générés pour rejouer sur une nouvelle graine.

## Risques à surveiller

- **Vouloir tout faire.** Une étape à la fois, jouable avant la suivante.
- **Les performances** : une planète plus grande, avec plus de vie et des saisons. Mesurer à chaque étape (l'écologie coûte ~4 ms par pas de vie).
- **La lisibilité** : une enquête sans indices justes devient de la devinette. Chaque anomalie doit laisser plusieurs indices convergents.
