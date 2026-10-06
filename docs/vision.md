# Vision

> Un druide apprend la magie d'une planète vivante en observant ceux qui la pratiquent, et il y vit longtemps.

Cette page est la boussole du projet (décidée en octobre 2026, « vision B »). Quand une idée ne la sert pas, elle attend. Le détail est dans `design.md`, les étapes dans `roadmap.md`, le contenu dans `catalogue/`.

## Le cœur

- **Une nature qui pratique la magie.** Les êtres vivants (cerfs, chouettes, lucioles, champignons…) ont des comportements étranges, liés à un lieu, une date, une phase de lune. Cette magie, ce sont des structures dissipatives : des synchronisations, des fronts, des spirales, des oscillations.
- **Le joueur enquête.** Il remarque, comprend quand et où, se cache, observe. Comprendre une anomalie lui apprend un sort ou une forme.
- **Il devient druide.** Chaque forme apprise le rapproche de la nature et l'éloigne des humains : **les mots du carnet deviennent des signes**. Ce n'est pas une punition mais une trace, en partie réversible : le joueur choisit jusqu'où aller.
- **Il vit longtemps.** Saisons et années, un refuge à bâtir, une terre à soigner, des liens avec les bêtes. Le jeu se joue comme une vie, pas comme une liste à terminer.

## Les principes

1. **La nature est plus forte que le joueur.** Il pousse, il ne commande pas.
2. **Le joueur est la cause sans le vouloir.** Observer dérange, agir abîme : les écosystèmes basculent (hystérésis), et une anomalie meurt avec son lieu.
3. **La magie naît de la vie.** Les anomalies apparaissent là où un écosystème est sain depuis longtemps, et disparaissent quand il se dégrade. Soigner un lieu peut en faire naître une.
4. **La fonction naît de la physique.** Pas de recettes : la matière a des propriétés et garde son histoire.
5. **Chaque animal façonne la terre.** Aucun animal n'est un décor : chacun transporte, creuse, plante, broute, piétine, bâtit, et le paysage en garde la trace. L'écureuil enterre ses glands à l'automne pour l'hiver ; ceux qu'il ne retrouve pas deviennent des chênes. La forêt est plantée par les bêtes.
6. **Tout est procédural.** Aucune texture, aucun modèle dessiné : l'apparence découle de l'état du monde.

## Le monde

- **Une planète finie qui se referme sur elle-même** : en marchant assez longtemps vers l'est, on revient par l'ouest. Grande (plusieurs heures pour en faire le tour), mais sans « ailleurs » infini : les conséquences restent.
- **Le temps long** : jours, lunes, saisons, années.
- **La source** : un grand cycle, une onde lente qui parcourt la planète. Toutes les anomalies en sont des rides locales et battent à son rythme. Le joueur peut finir par la comprendre et **prédire la grande marée**, un rendez-vous qui revient, plus fort quand la planète est saine.

## Ce qui a été écarté ou mis au second plan

- **Le laboratoire comme moteur** (la verrerie, la chimie stricte) : abandonné. Son esprit reste, puisqu'on reconnaît dans le paysage les motifs de la chimie (une spirale de Belooussov-Jabotinski dans un marais). `crates/sim` sert à animer les anomalies.
- **Le monde infini** : remplacé par la planète finie.
- **La survie** : une texture, jamais un enjeu (on ne meurt pas).

## Les rythmes du joueur

| Échelle | Ce qu'il fait |
|---|---|
| une heure de jeu | marcher, cueillir, faire du feu, remarquer quelque chose d'étrange |
| une lune (8 jours) | suivre une piste, revenir à la bonne nuit, observer un rituel, apprendre un sort |
| une saison | bâtir et agrandir son refuge, voir la nature changer (brame, neige, naissances) |
| une année | soigner une terre qu'il a abîmée, voir naître ou mourir des anomalies, attendre la grande marée |
