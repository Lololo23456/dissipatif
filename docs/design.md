# Document de game design

Version de travail. Les choix encore ouverts sont marqués « à trancher » et suivis dans `decisions.md`.

## Vision

Un jeu d'exploration contemplative dans un monde procédural très vivant. Le joueur est un naturaliste : il arrive dans un monde inconnu, l'observe, et cherche à comprendre ce qui l'entoure. La faune et la flore ont des comportements crédibles, et même les plantes semblent vivantes. À côté d'elles existe une **autre forme de vie**, les anomalies : des êtres qui ne sont pas faits comme les nôtres, et qui n'existent que parce qu'un flux d'énergie et de matière les traverse.

Le jeu mise d'abord sur l'**ambiance** : une lumière soignée qui change au fil de la journée, un ciel étoilé la nuit, un monde qui bouge et qui respire.

Chaque comportement du monde est un vrai phénomène (nuées, cycles prédateurs-proies, structures dissipatives), si bien que comprendre le monde, c'est développer une intuition des systèmes vivants et de la thermodynamique de Prigogine.

## Piliers

1. **Le monde est vivant.** Tout bouge, réagit, naît et meurt : animaux, plantes, eau, anomalies. Un monde immobile est un échec.
2. **Comprendre, c'est progresser.** La progression vient de l'observation, des découvertes notées dans le carnet, et des instruments qui rendent visible ce qui était caché.
3. **L'étrange n'existe que par contraste avec le normal.** La base est naturaliste et crédible ; les anomalies sont rares, et c'est parce que le joueur a appris le normal qu'il remarque l'anormal.
4. **Tout est généré.** Terrain, climat, biomes, plantes, créatures, ciel, couleurs et son découlent de règles et d'une graine. La direction artistique vit dans le choix des règles, des palettes et des paramètres.
5. **Interagir avec tout, par des règles et non des cas particuliers.** Quelques propriétés et quelques règles générales produisent des milliers d'interactions que personne n'a écrites.
6. **Le temps a une flèche.** Pas de retour en arrière gratuit : un feu, une rivière qui change de cours, une anomalie qui s'éteint laissent une trace.

## Le joueur

- Un personnage vu de haut, caméra plongeante à la manière de Minecraft Dungeons.
- Il explore, observe, note, manipule, construit. Il n'est pas un conquérant : il est un observateur qui apprend, et dont les actions ont des conséquences sur un monde qui réagit.
- **Il agit directement sur le monde** : détruire des blocs, creuser, en poser, abattre un arbre, déplacer de la terre ou des pierres, détourner un ruisseau. Un bloc détruit se brise en micro-cubes qui tombent et roulent. Le monde réagit en chaîne : l'eau s'engouffre dans un trou, un arbre abattu prive la terre de racines, une anomalie privée de son flux s'éteint.

## Boucle centrale

Explorer → observer (un animal, une plante, une anomalie, le ciel) → noter et relier dans le carnet → comprendre un comportement ou une règle → débloquer un instrument ou une action → explorer plus loin, voir ce qui était caché.

## Le monde

Généré à partir d'une graine (crate `world`) : la forme des terres (île, archipel, côte), le relief, le climat et les biomes.

- **Biomes** : mer, plages, déserts à dunes, savanes, prairies, forêts de feuillus, forêts de conifères, montagnes rocheuses, sommets enneigés.
- **Eau** : mer, lacs dans les cuvettes, rivières qui descendent vers la mer avec leurs affluents.
- **Plus tard, un monde qui change** : l'eau qui coule et érode, les rivières qui creusent et migrent (méandres), la végétation qui pousse et recule, les saisons et la météo.
- **Événements** : des phénomènes rares et marquants qui transforment le monde et laissent des traces durables : tornades qui couchent une forêt sur leur passage, arbres qui tombent (vieillesse, tempête, foudre), crues qui déplacent une rivière, incendies de forêt et repousse, éboulements en montagne. Ce sont eux aussi des structures loin de l'équilibre (une tornade est une structure dissipative), et le joueur apprend à les lire et à les anticiper.

## La vie

### Faune
Des créatures procédurales : chaque espèce est un « plan » (corps, pattes, tête, proportions, couleurs) tiré de la graine du monde.
- **Animation procédurale** : pattes qui suivent le sol, corps qui ondule, tête qui se tourne vers ce que l'animal regarde. Aucune animation dessinée.
- **Comportements émergents, issus de vrais modèles** :
  - nuées d'oiseaux, bancs de poissons, essaims de lucioles : trois règles locales (se rapprocher, s'aligner, s'éviter), le modèle des *boids* de Reynolds ;
  - prédateurs et proies : des populations qui oscillent (Lotka-Volterra), si bien que le monde n'est jamais deux fois pareil ;
  - migrations et rythmes liés au climat, au jour et à la nuit.
- Les animaux réagissent au joueur (fuite, curiosité, méfiance) : l'observation se mérite.
- **Ordre de construction** : d'abord les créatures sans pattes et très atmosphériques (oiseaux, poissons, lucioles), puis les animaux qui marchent.

### Flore
Des plantes qui semblent vivantes :
- elles se tournent vers le soleil, se ferment la nuit et s'ouvrent à l'aube ;
- certaines se rétractent quand on passe près (comme le mimosa pudique) ;
- le feuillage ondule au vent et « respire » très lentement ;
- certaines luisent la nuit.

L'effet recherché : on n'est jamais tout à fait sûr de ce qui est animé, ce qui brouille la frontière avec les anomalies.

### Les anomalies : une autre forme de vie
Ni animaux ni plantes : des structures dissipatives vivantes. Pas de cellules ni d'ADN, mais des motifs qui se nourrissent de gradients (chaleur, humidité, lumière, minéraux, eau qui coule) et qui s'éteignent quand le flux s'arrête. C'est la thèse de Prigogine prise au pied de la lettre : la vie comme ordre maintenu loin de l'équilibre.

- Elles naissent là où il y a un flux : source chaude, bord de rivière, versant ensoleillé.
- Elles poussent, se divisent, s'étendent, oscillent, meurent, selon de vraies règles de réaction-diffusion (Gray-Scott et d'autres, voir `docs/reactions/`).
- Elles coexistent avec la vie normale et entrent en relation avec elle : elles consomment la même eau, la même lumière ; certains animaux les évitent, d'autres s'en nourrissent.
- La question « est-ce vivant ? » est au cœur du mystère.

Pistes pour agir sur elles, héritées de la version précédente du design (toutes à confirmer, voir « À trancher ») :
- **Flux** : détourner une eau, une chaleur, ouvrir ou fermer un passage change ce dont elles vivent ; selon le flux émergent cellules, labyrinthes, spirales, oscillations, ou rien.
- **Bifurcations et ralentissement critique** : près d'un point de bascule, une anomalie récupère de plus en plus lentement d'une perturbation et ses fluctuations augmentent ; le jeu le rend perceptible (tremblement, instabilité sonore).
- **Hystérésis** : rétablir les conditions d'avant ne restaure pas l'anomalie d'avant.
- **Couplages** : deux anomalies reliées par un flux s'influencent ; les déchets de l'une nourrissent l'autre, l'instabilité se propage.
- **Cultiver** : l'idée de départ du projet, faire grandir et entretenir des anomalies en réglant leurs flux, reste possible comme mécanique ou comme but.

## Interactions systémiques

Chaque élément du monde a quelques propriétés (combustible, comestible, creusable, transportable, conducteur de chaleur, absorbant…) et quelques règles générales les relient :
- le bois brûle, l'herbe sèche brûle plus vite, l'eau éteint ;
- les animaux mangent certaines plantes, fuient le feu et le bruit ;
- la terre se creuse et se transporte, l'eau suit la pente ;
- les anomalies consomment un flux ; le couper les affame.

Le monde doit réagir de façon **cohérente** : c'est ce qui rend la compréhension possible.

## Ambiance

### Lumière
- Cycle jour-nuit : aube froide, midi lumineux, golden hour, crépuscule, nuit bleutée. Revoir le même lieu sous une autre lumière le rend nouveau.
- Ombres portées, ciel en dégradé avec disque solaire, brume qui prend la couleur du ciel.
- Direction artistique : aplats de couleur, palette chaude et restreinte, soleil chaud et ombres froides (référence : Firewatch).

### La nuit
- Ciel procédural : étoiles de couleurs et d'éclats variés, voie lactée, constellations propres à chaque monde, une ou plusieurs lunes avec leurs phases, peut-être des aurores ou un phénomène céleste lié aux anomalies.
- Vu de haut, le ciel se voit surtout **dans les reflets** : étoiles et lune dans les lacs et la mer.
- La nuit change le monde : d'autres créatures sortent, des plantes et des anomalies luisent.

### Mouvement
Rien n'est figé : feuillage au vent, eau qui bouge, pollen et poussière dans la lumière, nuées dans le ciel.

### Son
Procédural et spatial : vent dans les conifères, rivière qu'on entend en s'approchant, oiseaux en forêt, silence du désert. Les anomalies ont leur propre son : un régime stable bourdonne, une oscillation rythme, l'approche du chaos devient dissonante.

## Progression

- **Carnet** : observations, croquis, liens entre les faits (« les cerfs descendent boire au crépuscule », « les oiseaux fuient avant l'orage », « cette anomalie meurt quand la source gèle »).
- **Instruments** à construire, chacun révélant une couche cachée du monde :
  - le **télescope** : le ciel, les étoiles, les constellations ;
  - des instruments pour voir les flux (chaleur, eau), les rythmes d'une anomalie, les signes d'une bifurcation avant qu'elle n'arrive.

## Rendu

- Deux résolutions : le terrain en gros cubes (lisibilité, style), le vivant et les détails en petits cubes (4×4×4 par cube : plantes, fleurs, créatures, objets, motifs des anomalies). Le minéral est massif, le vivant est délicat. On n'affine pas tout le terrain : ce serait 64 fois plus lourd.
- Apparence fonction de l'état : matériau, profondeur de l'eau, état d'une anomalie.
- La lisibilité ne repose jamais sur la seule teinte : luminance, mouvement et son portent aussi l'information.

## Dimension recherche

Le jeu sert aussi de laboratoire : les régimes d'anomalies découverts (paramètres, conditions initiales, métriques) peuvent être enregistrés pour explorer l'espace des systèmes dissipatifs.

## À trancher

- **Ce que le joueur cherche, au fond** : comprendre les anomalies, les protéger, les cultiver, ou simplement les observer. C'est ce qui donnera le fil du jeu.
- La place exacte des mécaniques héritées (flux, bifurcations, hystérésis, culture) dans le rapport aux anomalies.
- Difficulté : un monde qui peut se dégrader sérieusement par les actions du joueur, ou un monde apaisé.
- Particules purement visuelles ou porteuses de matière (conservation).
- La forme des anomalies dans le monde : motifs dans le sol, êtres qui se déplacent, les deux.
