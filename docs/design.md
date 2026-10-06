# Document de game design

Version de travail. Les choix encore ouverts sont marqués « à trancher » et suivis dans `decisions.md`.

## Vision

Un jeu d'exploration contemplative dans un monde procédural très vivant. Le joueur est un naturaliste : il arrive dans un monde inconnu, l'observe, et cherche à comprendre ce qui l'entoure. La faune et la flore ont des comportements crédibles, et même les plantes semblent vivantes.

Le cœur du jeu est un **va-et-vient entre le monde et le laboratoire** : on prélève dehors, on fait réagir dans la verrerie, on voit naître des structures dissipatives (spirales de Belooussov-Jabotinski, anneaux de Liesegang, cristaux, bandes de Turing dans un gel), on comprend, et on retourne dehors avec une hypothèse.

## Critère d'authenticité (assoupli)

**Le monde physique est réel** : matières, feu, chaleur, réactions, écologie suivent de vraies lois, et les structures dissipatives sont de vraies structures (Belooussov-Jabotinski, Liesegang, Turing). **Par-dessus, une part de surnaturel** : les sorts (voir « Les comportements étranges et les sorts »). Ce n'est plus « tout ce que le joueur voit existe vraiment » : la règle devient « le monde obéit à ses lois ; le surnaturel est rare, cohérent, et se mérite par l'observation ».

## Le noyau

Un jeu de terrain où le joueur comprend un monde fragile en reproduisant ses structures dans une verrerie. La boucle : prélever dehors, reproduire au labo, déduire, retourner dehors. Le moment fort : reconnaître le même motif dans le gel et dans le paysage.

**La raison d'avancer, c'est l'écologie** : une structure dissipative meurt quand son flux est coupé, et rétablir les conditions d'avant ne la ramène pas (hystérésis). La disparition n'est pas scénarisée, elle découle de la thermodynamique.

**Le verbe central : le joueur est la cause, sans le vouloir.** Prélever, expérimenter, faire du feu altèrent le monde : comprendre a un coût. Chaque prélèvement pèse. Cela rejoint « la nature amplifie les petites actions du joueur ».

## Les comportements étranges et les sorts

Les animaux (puis les plantes, les arbres, les rivières) ont parfois des **comportements anormaux**, autour de certaines choses ou à certains moments. Le joueur qui les **observe assez longtemps** gagne un **sort** lié à cette anomalie de comportement. Observer est le seul moyen d'en obtenir : la patience du naturaliste est récompensée.

À trancher : la nature des sorts, et s'ils obéissent eux aussi à la grammaire des flux (un sort qui déplace de la chaleur, de l'eau, de la lumière, et donc a un coût et des conséquences écologiques) pour rester cohérents avec le reste du monde.

## Piliers

1. **Le monde est vivant.** Tout bouge, réagit, naît et meurt : animaux, plantes, eau, anomalies. Un monde immobile est un échec.
2. **Comprendre, c'est progresser.** La progression vient de l'observation, des découvertes notées dans le carnet, et des instruments qui rendent visible ce qui était caché.
3. **L'étrange n'existe que par contraste avec le normal.** La base est naturaliste et crédible ; les anomalies sont rares, et c'est parce que le joueur a appris le normal qu'il remarque l'anormal.
4. **Tout est généré.** Terrain, climat, biomes, plantes, créatures, ciel, couleurs et son découlent de règles et d'une graine. La direction artistique vit dans le choix des règles, des palettes et des paramètres.
5. **Interagir avec tout, par des règles et non des cas particuliers.** Quelques propriétés et quelques règles générales produisent des milliers d'interactions que personne n'a écrites.
6. **Le temps a une flèche.** Pas de retour en arrière gratuit : un feu, une rivière qui change de cours, une anomalie qui s'éteint laissent une trace.

## Principe directeur : la nature est plus forte que le joueur

Toute mécanique qui garde la nature dominante reste ; les autres sortent.

- **Le joueur pousse, il ne commande pas** : ses actions sont petites, et la nature les amplifie, les absorbe ou les annule.
- **Rien n'est permanent sans entretien** : c'est la logique des structures dissipatives, qui s'effondrent sans flux.
- **Le monde suit son cours** sans attendre le joueur.
- **La progression élargit la perception** (lentilles, télescope, carnet), jamais la puissance.
- **La survie rappelle que le naturaliste est fragile.**
- **L'échec se lit après coup** (carnet, historique) : le joueur doit se sentir petit, pas puni.

### Tout est touchable, rien n'est contrôlable

- Pas d'actions codées objet par objet, mais des **propriétés** (masse, dureté, inflammabilité, humidité, valeur nutritive…) et quelques **verbes généraux** : pousser, porter, jeter, enflammer, arroser, déplacer, goûter.
- **À l'échelle d'un humain** : un rocher oui, une montagne non.
- Chaque action **déborde en conséquences** que le joueur n'avait pas prévues ; la nature répare, détourne ou déforme.
- Toutes les actions passent par la **commande unique** (contrainte multijoueur, voir `decisions.md`).

### Le creusage

Il existe, mais pas à la Minecraft :
- il a un **coût** (temps, effort, dureté du matériau) et reste borné par le corps ;
- la **matière est conservée** : la terre est dans les mains ou en tas ;
- **la nature réagit** : l'eau s'accumule dans le trou, un sol sans soutien s'éboule, les racines tiennent le terrain, le trou se comble avec le temps ;
- techniquement, il demande un remaillage local : à faire avec la simulation de l'eau et des éboulements, pas avant.

### L'artisanat : la fonction naît de la physique

Pas de recettes, pas de menu, pas de structures prédéfinies. On **pose** des objets dans le monde, et une **simulation physique** décide de ce qu'ils deviennent (`docs/reactions/chaleur-combustion.md`) :
- des brindilles qui brûlent font un feu ; des pierres autour chauffent et gardent la chaleur ; enfermer le feu le rend plus chaud mais l'étouffe : un four émerge, personne ne l'a programmé ;
- le feu s'allume par friction : l'effort donne une **braise**, qui allume de l'**amadou**, qui allume le **petit bois** ;
- une argile modelée doit **sécher** ; humide dans le feu, elle **éclate** ; sèche et maintenue au-dessus de 600 °C, elle devient de la **terre cuite**.

**La matière garde son histoire** : son origine (argile de berge ou terre rouge de savane, riche en fer), ce qu'elle a subi (la plus haute température, le temps passé à cuire, l'eau qu'elle contenait). Les objets qui en sont faits en héritent, et les impuretés suivront plus loin dans la chaîne.

Le seul geste « de fabrication » est celui des mains : **modeler** une matière plastique. Tout le reste, c'est poser, attendre, observer.

## Le joueur

- Un personnage vu de haut, caméra plongeante à la manière de Minecraft Dungeons.
- Il explore, observe, note, manipule, construit. Il n'est pas un conquérant : il est un observateur qui apprend, et dont les actions ont des conséquences sur un monde qui réagit.
- **Il agit directement sur le monde, à l'échelle de son corps** (voir le principe directeur) : creuser un peu, déplacer de la terre ou des pierres, détourner un ruisseau, abattre un arbre au prix d'un effort. Ce qui est arraché se brise en micro-cubes qui tombent et roulent, et la matière est conservée. Le monde réagit en chaîne : l'eau s'engouffre dans un trou, un arbre abattu prive la terre de racines, une anomalie privée de son flux s'éteint.

## Boucle centrale

**Prélever dans le monde → analyser au labo → comprendre → retourner dehors avec une hypothèse.**

- **Prélever** : eaux, sels, minéraux, cendres, plantes, terres. Les matières varient selon le lieu (une source ferrugineuse, une saumure, un grès rubané), ce qui pousse à explorer.
- **Analyser** : dans la verrerie, les échantillons réagissent. La réaction-diffusion (Gray-Scott et les autres règles de `docs/reactions/`) est le moteur qui anime les récipients.
- **Comprendre** : les réactions sont sensibles à la température, aux concentrations, aux impuretés. Le carnet consigne les essais, les conditions et ce qui en sort.
- **Retourner dehors** : une hypothèse (« les anneaux de ce grès viennent du fer de cette source ») mène à un autre lieu, un autre prélèvement.

Ce qui existe déjà y sert : la survie, le sac et le ramassage servent à rapporter des échantillons.

## Le laboratoire

- **Une petite île de contrôle** dans un monde plus fort que le joueur : on y règle ce qu'on peut, mais les réactions restent sensibles et capricieuses, et les matières premières viennent du dehors.
- **La progression, c'est le labo** : verre, creuset, filtre, balance, source de chaleur… chaque instrument ouvre de nouvelles expériences, jamais de la puissance.
- Les phénomènes visés, tous réels : réaction de Belooussov-Jabotinski (ondes en spirale), anneaux de Liesegang (précipitation périodique dans un gel), cristallisation, motifs de Turing dans un gel.

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

### Les anomalies (version précédente, remplacée)
**Remplacée** par le critère d'authenticité et le laboratoire : les structures dissipatives reviennent dans la verrerie et dans les phénomènes naturels réels, pas sous forme d'êtres inventés. Le texte ci-dessous est gardé pour mémoire.

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

- La place exacte des mécaniques héritées (flux, bifurcations, hystérésis, culture) dans le rapport aux anomalies.
- Particules purement visuelles ou porteuses de matière (conservation).
- La forme des anomalies dans le monde : motifs dans le sol, êtres qui se déplacent, les deux.
