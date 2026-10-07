# Phase 2 — La tranche verticale

Environ deux heures de jeu complètes, d'une traite. Ce document détaille chaque point, ce qui est décidé (**décidé**), ce que Claude propose en attendant (**proposé**, à valider) et ce qui reste ouvert (**à trancher**). Les décisions passent ensuite dans `decisions.md`.

## Le cadre de la tranche

- **On ne passe pas la nuit** (décidé). Une journée dure 20 min réelles : deux heures font environ **six jours de jeu**, de la fin de l'été au début de l'automne, entre une pleine lune (nuit 3) et une nouvelle lune (nuit 7). Les nuits sont du contenu, pas un temps mort.
- **L'accélération du temps (T)** reste pour le développement ; elle sera retirée du jeu plus tard (décidé).
- **Tout est tiré de la graine**, météo comprise (décidé).
- **Les anomalies sont de tous types et énigmatiques** (décidé) : pas trois fois « des êtres en cercle ». Elles varient par ce qui les porte (animaux, eau, sol, pierre, son…), leur forme (synchronisation, front, spirale, taches…), leur rapport au temps (périodique, déclenchée, lente, fugace) et au joueur. Une anomalie peut en désigner une autre.
- Règles pour l'énigme (proposé) : on voit la conséquence avant le phénomène ; l'indice ne nomme pas la cause ; il faut souvent deux conditions qui coïncident ; l'explication physique existe mais reste cachée ; des fausses pistes naturelles ; au moins trois indices qui convergent par anomalie.

### Le fil proposé

1. **Le rituel des cerfs** (pleine lune, nuit 3) → la Forme du cerf ; le carnet perd ses premiers mots.
2. **Le regard des chouettes** (nuits sombres, autour de la nuit 7) : toutes fixent le même point ; il faut croiser leurs regards sur la carte du carnet. → l'Œil de chouette.
3. **La spirale du marais** : c'est ce que les chouettes pointent ; l'Œil la fait entendre et voir la nuit.
4. **La fin** : le point fixé par les chouettes se déplace nuit après nuit ; c'est l'onde de la source qui passe.

## 4. Le cadre des anomalies émergentes

Une anomalie est décrite par : son support, son lieu (choisi par l'état du monde), son moment (heure, lune, saison, météo), ses conditions de vie, ses présages, ses indices, son épreuve, ce qu'elle apprend. Cycle : dormante → vivante → active à ses moments → déclinante → morte.

- **Santé du lieu** (décidé), à long terme : à quel point la nature va bien à cet endroit. Une mesure par lieu (couverture végétale, eau et humus du sol, nombre d'individus de l'espèce), lissée sur plusieurs jours, avec deux seuils : naissance au-dessus du haut, mort sous le bas (hystérésis).
- **Force de l'anomalie** (décidé), à court terme : tant que ses conditions sont réunies, l'anomalie reste. Une force entre 0 et 1 baisse quand le joueur la dérange ou abîme son lieu (repéré, piétinement, feu, creusement) et remonte lentement d'elle-même tant que la nature va bien. Une anomalie faible se manifeste moins (moins d'êtres, plus court, présages plus rares).
- **Épreuves variées** (décidé) : parfois se rendre à un lieu, parfois suivre une piste, parfois faire une action.
- **Où** (décidé) : au moins une anomalie de chaque sorte de la tranche est garantie près du départ ; ensuite, partout où les conditions sont réunies.
- **Pas de blocage** (décidé). Règle d'or : **la magie est un raccourci, la connaissance du milieu est la fondation**. Toute épreuve qu'un sort facilite reste faisable sans lui, par une approche physique plus exigeante (le vent, le couvert, l'affût, la discrétion). Une anomalie morte renaît quand ses conditions reviennent ; ses traces restent un temps ; une anomalie ratée revient la nuit suivante, plus faible.
- À trancher : comprendre exige-t-il d'avoir relié les indices dans le carnet ?

## 5. Le marais et sa spirale

- Le marais n'existe pas encore dans `crates/world` : un milieu d'eau stagnante peu profonde et riche (roseaux, algues, lentilles d'eau), garanti à portée de marche de la prairie.
- `Oregonator::step` : **exercice de l'utilisateur** (règle `simulation.md`), puis tests (repos stable, onde, spirale) et balayage des régimes dans `lab`.
- Liaison proposée : une grille 2D sur la surface du marais ; v colore l'eau et les algues ; la température multiplie dt (Arrhenius) : la spirale ralentit à l'automne. Elle meurt si l'eau se trouble (cendres, terre lessivée) ou si le marais s'assèche.
- À trancher : ce qui fait naître la spirale (une onde rompue, mais par quoi) ; visible le jour ou seulement la nuit avec l'Œil ; son son ; apprend-elle un sort (aucun, ou le Feu follet) ; taille de la grille selon le coût.

## 6. Le rituel des cerfs dans ce cadre

- **Décidé** : au départ, le monde est à l'équilibre, le rituel est déjà vivant. **Si le rituel meurt, la Forme du cerf disparaît aussi.**
- Proposé : il naît là où une harde d'au moins N cerfs vit sur une prairie saine depuis une durée D ; il meurt si la harde s'amaigrit ou si la prairie est brûlée ou mise à nu.
- **Décidé** : le sort revient quand le rituel renaît (la prairie a repoussé, la harde se porte bien). L'Œil de chouette s'apprend aussi sans la Forme du cerf, plus difficilement : accroupi, contre le vent, depuis un affût. Le joueur doit protéger le lieu qui lui a appris son sort.
- **Décidé** : la transformation est irréversible. Perdre un sort ne rend pas les mots effacés : on devient un druide amputé de sa magie. Dans le carnet, le signe de la forme perdue se fissure, grisaille, paraît éteint ; le mot d'origine reste illisible.
- À trancher : les seuils N et D ; le rituel peut-il déménager vers une autre prairie ?

## 7. Les feuilles et la litière

- **Décidé** : le monde doit être très interactif ; la litière se ramasse ; la décomposition suit la vraie vie.
- **Décidé** : un stock de litière par parcelle de sol (8 × 8) qui se décompose en humus N selon la chaleur et l'humidité, à la vitesse réelle rapportée à l'année du jeu (64 jours) : environ une à deux années de jeu pour une feuille de chêne. Dans la tranche, on ne voit pas l'humus se former, mais les feuilles s'accumuler et changer de couleur.
- Proposé : des feuilles visibles qui tombent avec le vent (près du regard) ; les feuilles tombées dans l'eau enrichissent le marais.
- Probablement (décidé) : les pas font du bruit dans les feuilles : on entend venir les bêtes, et elles entendent le joueur.

## 8. La vie quotidienne minimale

- **Décidé** : une construction libre par **assemblage modulaire sur la grille voxel**, sans physique complexe (pas de centre de gravité ni de portance). Règles de support simples : un poteau touche le sol ; un plancher touche un poteau ; un toit touche un mur ou un poteau. Si un support disparaît (le bois pourrit ou brûle), ce qui est au-dessus tombe. Matériaux de la tranche : bois brut, argile, chaume. Bâtir est lent et apaisant ; ce n'est pas le cœur du jeu.
- Proposé : l'affût d'abord (il sert l'enquête), puis une cabane (abri de pluie et de froid, foyer, rangement ; pas de lit), cuisiner par la physique (noisettes grillées, glands lessivés, champignons), récolter (cueillette d'automne), semer des glands et des noisettes (ils germeront au printemps).
- À trancher : la force de la faim ; le rangement.

## 9. Le carnet qui perd ses mots

- **Décidé** : une part des mots se défait à chaque forme apprise, les mots abstraits d'abord (le temps, les verbes), les noms concrets en dernier (arbre, cerf) : la syntaxe humaine se perd. Perdre un sort ne rend pas de mots (voir le point 6).
- **À reprendre** : la roadmap prévoyait que rester humain longtemps rende des mots ; à confirmer avec l'irréversibilité décidée au point 6.
- Proposé : métadonnées en signes (lune, vent en flèche, heure par le soleil, petite carte) ; liens entre pages ; carte où le carnet note seul la direction du regard d'une chouette observée, et où le joueur croise les traits.
- À trancher : la part de mots par forme ; le joueur peut-il annoter.

## 10. La roue des sorts, et le trio chouette, renard, campagnols

- Proposé : la roue (maintenir, viser, relâcher), deux sorts dans la tranche ; les campagnols en densité par parcelle (graines, herbe, galeries visibles), quelques individus visibles de près ; un ou deux renards (mulotage, cris, terrier, traces) ; des hulottes (territoires, chasse à l'oreille, cris) ; l'anomalie du regard (point fixé, chaîne de hululements, pelotes tombées du même côté, silence soudain) ; l'Œil de chouette comme sort de perception (voir dans le noir, entendre ce qui bouge, ébloui par le feu), pas comme forme volante dans la tranche.
- À trancher : garder aussi le cercle autour de l'arbre mort ; l'épreuve de l'Œil.

## 11. Des saisons qui comptent

- **Décidé** : tout tiré de la graine ; complet (lumière, météo et sons).
- **Décidé** : la partie commence à la fin de l'été. En six jours : la lumière qui devient plus rasante et dorée, les premières brumes du matin sur le marais, les insectes de nuit qui se taisent peu à peu, les premières feuilles qui tombent.
- Proposé en plus : la longueur du jour selon la saison ; des pluies d'automne ; le brame, les hulottes plus bavardes à l'automne, le renard.

## 12. Une fin provisoire

- Proposé : le point fixé par les chouettes dérive nuit après nuit ; la dernière nuit, avec l'Œil, une onde lente traverse le paysage et la spirale bat avec elle.
- À trancher : continue-t-on à jouer après ; que voit-on exactement ; date fixe ou quand le joueur a compris l'Œil.

## Ordre de travail proposé

1. Le cadre et le rituel (4, 6). En parallèle : `Oregonator::step` (chemin critique).
2. Le carnet en signes et la roue (9, 10).
3. Le marais et la spirale (5).
4. Le trio et le regard des chouettes (10).
5. Les feuilles, la lumière, les sons (7, 11).
6. L'affût, la cabane, la cuisine (8).
7. La fin (12).
