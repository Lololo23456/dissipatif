# Journal des décisions

Une entrée par décision. Statut : **actée** ou **provisoire** (à revoir quand le prototype en dira plus).
Format : date, décision, raison, statut.

## 2026-10-05 — Rust + wgpu, sans moteur de jeu
Contrôle total sur les compute shaders, la simulation est le cœur du projet. **Actée.**

## 2026-10-05 — Simulation d'abord sur CPU, GPU ensuite
Un bug numérique se trouve bien plus facilement en Rust qu'en WGSL. La version CPU reste la référence de vérité pour valider la version GPU. **Provisoire.**

## 2026-10-05 — Construire la simulation avant le moteur voxel
Le premier prototype doit prouver que la mécanique est prenante avant d'investir dans le rendu. **Actée.**

## 2026-10-05 — Style Minecraft Dungeons, cubes subdivisibles
Caméra plongeante et zones limitées : moins de distance d'affichage, plus de budget pour la simulation. Cubes macro subdivisibles en micro-cubes pour les particules. **Actée.**

## 2026-10-05 — Facteur de subdivision 4×4×4
64 micro-cubes par cube : assez pour des éclats lisibles, mémoire raisonnable. Passer à 8×8×8 seulement si le rendu le justifie. **Provisoire.**

## 2026-10-05 — Particules visuelles au départ
Les micro-cubes sont d'abord décoratifs. Les structures de données gardent la quantité de matière portée par chaque particule pour permettre plus tard la conservation de la matière. **Provisoire.**

## 2026-10-05 — Claude explique, l'humain écrit le cœur de la simulation
Objectif d'apprentissage. Dans `crates/sim` et les compute shaders, Claude explique et relit ; il n'écrit l'implémentation que sur demande explicite. **Provisoire.**

## 2026-10-05 — Versions épinglées
wgpu 30, winit 0.30, glam 0.34, bytemuck 1, pollster 1. Une montée de version majeure de wgpu se fait dans un commit dédié. **Actée.**

## 2026-10-05 — Nom « Dissipatif »
**Provisoire.**

## 2026-10-05 — Socle de rendu minimal en parallèle de la simulation
Précise « Construire la simulation avant le moteur voxel » : on construit seulement ce que le prototype exige pour afficher une parcelle (wgpu, caméra plongeante, maillage des cubes macro, couleur, éclairage simple), testé sur un champ synthétique en attendant Gray-Scott. Briques micro 4×4×4, particules et instancing attendent la récolte. **Actée.**

## 2026-10-05 — Géométrie par occupation, couleur par texture 3D
La couleur de chaque voxel dépend d'un état continu : un maillage glouton qui fusionne par couleur ne fusionnerait presque rien. Le maillage ne dépend que de l'occupation (seuil sur le champ) ; le fragment shader lit le champ dans une texture 3D et applique la palette. Changer les couleurs coûte un envoi de texture, on ne remaille que si l'occupation change. On commence par un maillage des faces visibles (culled) ; le glouton seulement si les mesures le justifient. Mesure : 48³ maillé en 0,34 ms (7 900 faces), le glouton n'est pas nécessaire. **Provisoire.**

## 2026-10-05 — La forme et la couleur viennent de deux champs différents
Sur la surface visible de {champ > seuil}, le champ vaut presque partout le seuil (mesuré : 50 % des faces entre 0,500 et 0,525 pour un seuil de 0,5). Colorier une forme par le champ qui la définit ne montre rien. Avec Gray-Scott : la forme vient de v, la couleur d'un autre état (u, température, âge…). **Provisoire.**

## 2026-10-05 — Direction artistique : lumière dorée, référence Firewatch
Aplats de couleur, palette chaude restreinte (prune → or pâle), soleil chaud et ombres froides éclairées par le ciel (ambiante hémisphérique), occlusion ambiante par sommet, brume pêche avec la distance qui sert aussi de fond. La palette des concentrations reste dans L* ≈ 36 → 75 pour laisser la luminance à l'éclairage, qui porte la forme. Tout vit dans `palette.rs`. Répond en partie à « Univers et ambiance » de `design.md`. **Provisoire.**

## 2026-10-05 — Un terrain fixe et un bassin de mélange
Une réaction qui occupe tout le volume est illisible : aucun repère fixe, et la caméra ne voit que la surface d'un bloc. Le monde est un terrain inerte, généré une fois, et la réaction tourne dans un bassin peu profond creusé dedans (40×6×40), à parois étanches (Neumann, flux nul). La terre ne participe pas à la réaction pour l'instant ; la nourriture vient toujours du réservoir F. Côté rendu, terrain et bassin sont deux « volumes » (maillage, champ de couleur, palette, origine) dessinés par le même pipeline. **Remplacée** le jour même par « La terre est vivante » : le bassin restait un objet collé dans le décor.

## 2026-10-05 — La terre est vivante
La réaction ne vit plus dans un bassin séparé : elle tourne dans la couche de surface du terrain (grille 2D de 72×72, une cellule par colonne, parois étanches aux bords du monde) et se lit dans la couleur du sol. Le terrain ne change jamais de forme : maillé une fois, seul un champ « vie » est envoyé au GPU à chaque image. Le shader mélange la couleur de la terre (palette `earth`) et celle du vivant (palette `concentration`) selon v : terre teintée au bord d'une tache, or au cœur, plus clair que la terre (lisible sans la teinte). Limites assumées pour l'instant : la diffusion ignore le relief, et la terre ne nourrit pas la réaction (F uniforme). **Provisoire.**

## 2026-10-05 — D'abord un monde vivant : eau, érosion, forêts
La réaction (Gray-Scott, « l'anomalie ») est mise de côté dans le jeu ; son code, ses tests et sa fiche restent dans `crates/sim`, et l'état « terre vivante » est dans le commit 472e8e3. Le monde lui-même devient dynamique, en trois phases : (1) terrain en carte de hauteurs continues + eau (modèle des tuyaux virtuels, Mei et al. 2007 : source, pluie, évaporation, bords ouverts vers la mer) ; (2) érosion et dépôt, la rivière change de cours ; (3) végétation couplée à l'eau, dont les racines freinent l'érosion. Rivières, deltas et fronts de forêt restent des structures dissipatives : on garde l'esprit du projet. Affichage : l'eau peu profonde teinte le sol (mélange base + vie), l'eau profonde devient des voxels. **Provisoire.**

## 2026-10-05 — Érosion, méandres, eau transparente et particules
- **Érosion** (Mei et al.) : capacité de transport ∝ pente · vitesse · min(1, profondeur / d_ref). Sans le facteur de profondeur, la pellicule de pluie, rapide mais infime, rasait tout le monde (mesuré : −146 cellules en 6 min). Éboulement au-delà d'un talus pour éviter puits et aiguilles.
- **Transport du sédiment par les tuyaux** de l'eau (même fraction que l'eau qui part), au lieu du schéma semi-lagrangien de Mei : celui-ci perdait 8 % de la matière ; le nôtre la conserve exactement en monde fermé.
- **Méandres** : la rive extérieure d'un virage perd de la matière ∝ courbure · vitesse, déposée à l'identique côté intérieur. Mesuré : environ la moitié du lit se déplace toutes les 2 minutes de jeu.
- **Eau** : surface transparente à la hauteur exacte de l'eau (et non plus des voxels arrondis, invisibles pour un ruisseau de 30 cm), opacité selon la profondeur, écume sur les rapides, rides animées, reflet du soleil. Volume réel : le joueur pourra y entrer.
- **Particules** d'écume et d'embruns : cubes en instancing, mis à jour sur CPU (version GPU plus tard, selon la règle CPU d'abord). Décoratives : elles lisent l'eau sans la modifier.
Coût mesuré : 7,7 ms CPU par image (simulation 96×96 à 20 pas par image + affichage + particules). **Provisoire.**

## 2026-10-05 — Une montagne d'où viennent les rivières
Le terrain n'est plus une pente générale : un massif au fond du monde (sommet ≈ 25 cellules), ses flancs taillés de crêtes et de vallons par un bruit « en arêtes », une plaine qui s'en éloigne. Les sources ne sont plus à des coordonnées fixes : elles sont placées au point le plus bas d'un anneau autour du sommet, côté plaine, donc dans un vallon où l'eau se rassemblerait. Pluie orographique : jusqu'à 5 fois plus au sommet que dans la plaine. Au-dessus de 15 cellules, roche nue au lieu de l'herbe. **Provisoire.**

## 2026-10-05 — D'abord construire le monde, sans simulation
L'eau dynamique et l'érosion donnaient un résultat illisible (vu enfin grâce à l'outil de capture : brume écrasante, bouillie de pixels, pas de vraies couleurs). On construit d'abord un monde statique, entièrement procédural ; la vie (eau qui coule, érosion, végétation qui pousse) reviendra par-dessus. Le code de simulation reste dans `crates/sim` ; les morceaux de jeu qui en dépendent sont dans `crates/game/parked/`, non compilés. **Actée.**

## 2026-10-05 — Un crate `world` pour la génération du monde
Ni simulation (`sim`), ni rendu : la génération est pure, déterministe et testable sans GPU. Étapes : continentalité (île, archipel ou côte selon la graine, toujours de la mer aux bords), collines, massifs en bruit d'arêtes ; climat (température : bruit + gradient nord-sud − altitude ; humidité : bruit + mer), contraste du bruit étiré car un bruit fractal reste près de 0,5 ; biomes de Whittaker + plages + ligne de neige ; dunes ; hydrographie par « priority flood » (lacs = cuvettes comblées, rivières = débit accumulé fort, lits creusés) ; végétation sur grille décalée aléatoirement (feuillus, conifères, acacias, cactus, buissons). Monde 256×64×256 généré en ~20 ms. **Provisoire** sur les réglages.

## 2026-10-05 — Couleurs par table de matériaux
Les voxels du monde portent un matériau (herbe, sable, roche, neige, bois, feuilles…), pas une valeur continue : un dégradé ne convient pas. Le champ de couleur stocke `id + variation` dans un f32 (partie entière : couleur dans une table de 16 matériaux, partie décimale : petite variation de luminosité). Les matériaux se distinguent aussi par leur luminance. **Actée.**

## 2026-10-05 — Outil de capture hors écran
`--capture` rend une image sans fenêtre (texture hors écran relue par le CPU, encodeur PNG minimal sans dépendance). Sert à vérifier visuellement chaque changement de rendu, y compris par Claude. **Actée.**

## 2026-10-05 — Vision : un naturaliste dans un monde très vivant
Le jeu devient d'abord une exploration contemplative : un personnage vu de haut observe et cherche à comprendre un monde procédural très vivant (faune aux comportements émergents, flore qui semble animée, cycle jour-nuit, ciel étoilé vu dans les reflets, télescope à construire). Les anomalies sont une autre forme de vie, des structures dissipatives qui coexistent avec la faune et la flore ; elles sont rares, pour que l'étrange se détache du normal. Interactions systémiques (propriétés + règles générales). Deux résolutions : terrain en gros cubes, vivant et détails en micro-cubes. Les mécaniques de la version précédente (flux, bifurcations, hystérésis, culture) deviennent des pistes pour agir sur les anomalies. Remplace la vision « jeu de gestion de parcelles » ; voir `design.md`. **Actée** pour la direction, ce que le joueur cherche au fond reste **à trancher**.

## 2026-10-05 — Ombres portées par carte d'ombre
Deux passes par image : la scène vue du soleil (projection orthographique ajustée au monde, profondeur seule, carte de 2048²), puis la vue caméra qui compare chaque point à la carte (filtrage PCF 3×3 pour des bords doux, décalage le long de la normale et biais de profondeur contre l'acné). Dans l'ombre, seul le soleil disparaît : la lumière du ciel reste, les ombres sont bleutées. L'eau ne projette pas d'ombre (transparente) mais en reçoit. Eau profonde plus opaque (0,95) pour que les ombres du fond marin ne brouillent pas la mer. **Actée.**

## 2026-10-05 — Mouvement : feuillage au vent et vie dans l'air
Le feuillage (feuilles, aiguilles) ondule dans le vertex shader, aussi dans la passe d'ombre : déplacement fonction continue de la position (deux sommets au même endroit bougent pareil, pas de fentes entre blocs), rafales lentes qui traversent le monde. Particules d'ambiance sur CPU autour de la cible de la caméra : pollen et poussière qui dérivent, feuilles qui tombent de sous les vrais feuillages. **Actée.**

## 2026-10-05 — Plantes en micro-voxels, dessinées par instancing
Chaque espèce (feuillu, conifère, acacia, cactus, buisson) a 6 variantes générées en micro-voxels (4 par cellule) : troncs fins, branches, houppiers en touffes irrégulières. Chaque plante du monde est une instance d'une variante, tournée d'un quart de tour : 30 modèles (~50 000 faces) au lieu de millions de faces. Les sommets des modèles portent leur matériau (bit `DIRECT_MATERIAL` du champ `cell`). Le monde garde une copie grossière des plantes en cellules (utile pour les interactions futures). Première application de la « double résolution » du design : terrain en gros cubes, vivant en micro-cubes. **Actée.**

## 2026-10-05 — Plantes toutes différentes, vent naturel
- Variété : 16 variantes de forme par espèce, 8 orientations (quarts de tour + miroir), une taille par plante (0,8 à 1,2 ; buissons 0,7 à 1,3) et une teinte de feuillage par plante (luminosité, chaleur ; 7 % des feuillus en couleurs d'automne). Générer un modèle unique par arbre coûterait environ 400 Mo de géométrie : on combine des sources de variation à la place.
- Vent, d'après « Vegetation Procedural Animation and Shading in Crysis » (GPU Gems 3, ch. 16) : flexion principale de toute la plante dans le sens du vent (facteur de flexion polynomial en hauteur, longueur conservée : la cime décrit un arc), autour d'une inclinaison et non de la verticale, sous des rafales lentes (périodes de 5 à 15 s) qui traversent le monde ; flexion de détail du houppier par ondes triangulaires lissées aux fréquences de Crysis (1,975, 0,793, 0,375, 0,193 Hz), phase variant lentement dans l'espace pour que chaque touffe bouge d'un bloc. Le déplacement ne dépend que de la position (pas du matériau) : aucune déchirure entre bois et feuilles. Souplesse par espèce (cactus rigide). Les deux premières versions (phase par sommet, frémissement rapide) donnaient un effet « gelée » et « bourré ». Les feuilles arrachées par les rafales partent dans le sens du vent.
- Les modèles ne subissent pas l'élimination des faces arrière : un miroir inverse le sens des triangles, ce qui trouait les arbres en miroir. **Actée.**

## 2026-10-05 — Tapis végétal et nouvelles espèces
15 espèces en micro-voxels : feuillu, conifère, acacia, cactus, buisson, bouleau, saule (au bord de l'eau : distance à l'eau calculée par parcours en largeur), palmier (plages), arbre mort ; et un tapis végétal tiré colonne par colonne, à position libre dans la cellule (deux tirages) : herbe, fleurs (4 couleurs), fougères, champignons, cailloux, broussailles sèches. Table de matériaux portée à 32. Seuls les arbres et buissons ont une copie grossière dans la grille. Souplesse au vent par espèce, plus grande pour les petites plantes (la flexion croît avec le carré de la hauteur). Herbe de savane teintée paille. Densités réglées à l'image. Environ 10 000 éléments au sol par monde, ~250 000 faces de modèles. **Provisoire** sur les densités.

## 2026-10-05 — Le naturaliste et ses déplacements
Personnage en pièces articulées de micro-voxels (≈ 1,85 cube) : chaque pièce (jambes, buste avec sac, tête et chapeau, bras) a son pivot et reçoit une matrice par image (nouveau type d'instance `PartInstance`, réutilisable pour les animaux). Animation procédurale : cycle de marche indexé sur la distance parcourue (les pieds ne glissent pas), balancier bras/jambes opposés, rebond à chaque pas, respiration à l'arrêt, buste penché en courant, tête qui regarde autour d'elle (curiosité). Contrôles par position physique des touches (ZQSD en AZERTY) relatifs à la caméra ; vitesse, rotation et caméra lissées exponentiellement ; montée automatique d'une marche avec affichage lissé ; saut ; marche ralentie puis nage dans l'eau ; collisions avec le sol et les troncs (feuillages et herbes traversables). Apparition sur un sol meuble d'un biome accueillant. Table de couleurs portée à 48 (vêtements). Survie choisie : faim, soif et température, en mode doux. **Actée.**

## 2026-10-05 — Nage, cailloux solides, petites plantes fines et qui s'écartent
- Nage : hystérésis (on nage au-delà de 1,3 cube d'eau, on arrête sous 0,8) ; sans elle, la flottaison faisait sortir de l'état « nage », le corps retombait et coulait. Flottaison stable, tête hors de l'eau ; Espace donne un coup de pied ; on se hisse sur une rive jusqu'à 1,6 cube. Testé : flotter, puis rejoindre la rive la plus proche et sortir.
- Cailloux du tapis végétal solides : boîtes rangées par colonne (`Obstacles`), les galets de moins de 0,2 cube se franchissent.
- Résolution par modèle : arbres à 4 voxels par cube, tapis végétal et buissons à 8 (brins d'herbe recourbés, fleurs à cinq pétales, frondes découpées, amanites tachetées).
- Les plantes souples (herbe, fleurs, fougères, buissons) se couchent en s'écartant du joueur qui les traverse (position du joueur transmise au shader), longueur conservée, et se redressent derrière lui. **Actée.**

## 2026-10-05 — Fleurs sauvages à leur échelle, flexion au contact
- Les fleurs étaient géantes : à 8 voxels par cube, un voxel fait ~12 cm (le naturaliste mesure 1,85 cube) et une corolle 40 cm. Elles passent à 16 voxels par cube (~6 cm), en touffes de tiges fines sur une rosette, quatre espèces : marguerite, coquelicot, lavande, bouton d'or.
- La flexion au passage était trop forte (≈ 40°, 1,1 cube de portée, toute la touffe d'un bloc). Désormais au contact seulement (0,75 cube), ≈ 25° au plus, chaque point selon sa propre distance au corps ; buissons plus raides. **Actée.**
- Correction : la flexion au passage utilisait seulement la direction pied → plante ; sous le pied, elle basculait d'un côté à l'autre en une image (saut mesuré de 0,41 cube, vu comme un « zoom »). Le déplacement est désormais proportionnel au vecteur pied → plante : nul sous le pied, continu autour (saut maximal mesuré : 0,05 cube par image).
- Remplacement : la flexion calculée dans le shader à partir de la position du joueur était sans mémoire ; la pointe des plantes suivait le joueur et tournait autour de leur pied (« effet 360 »). Désormais chaque plante touchée a une inclinaison simulée sur le CPU par un ressort amorti (`game/src/trample.rs`, raideur 90, amortissement 9) : poussée dans le sens de la marche et vers l'extérieur au contact, retour seule à la verticale une fois libre. Seules les plantes en mouvement sont simulées ; l'inclinaison passe par instance (`scale_mirror.zw`) et la plante entière plie depuis son pied. **Actée.**
- Caméra : inclinaison par défaut passée de 50° à 40° (plus rasante, plus cinématographique ; demande du joueur après essai à 58°).

## 2026-10-05 — Post-traitement et atmosphère
- La scène n'est plus dessinée directement à l'écran : elle va dans une image HDR (`Rgba16Float`), puis un passage plein écran (`render/src/post.rs`, `shaders/post.wgsl`) produit l'image finale : tilt-shift (bande nette au centre, flou en haut et en bas : effet miniature), bloom léger, étalonnage façon Firewatch (ombres violet-bleu, lumières ambrées, épaule douce sur les hautes lumières), vignettage, grain.
- Ombres de nuages : une couche de bruit fractal à 60 cubes de haut, lue le long du rayon vers le soleil et poussée par le vent ; elle n'atténue que le soleil (les ombres gardent le bleu du ciel).
- Poussières du monde lumineuses : elles brillent au soleil et scintillent (alpha de `ParticleInstance` = éclat), le bloom en fait de petites étincelles.
- Poussière de premier plan : disques flous (bokeh) dessinés dans le post-traitement, en deux couches avec parallaxe quand la caméra bouge. Purement visuelle, sans lien avec le monde. **Actée** ; réglages dans les constantes en tête de `post.wgsl`.
- Écartés pour l'instant : rayons de soleil volumétriques (coûteux, délicats).

## 2026-10-05 — Multijoueur possible plus tard
- **Provisoire** : un mode coopératif (2 à 4 joueurs, l'un d'eux héberge) pourrait venir à la fin. Aucun code réseau pour l'instant, mais l'architecture doit le permettre sans réécriture.
- Contraintes adoptées dès maintenant :
  1. Le jeu gère une liste de joueurs avec des identifiants, même s'il n'y en a qu'un.
  2. Toute action qui modifie le monde (ramasser, casser un bloc, poser, boire…) passe par une commande (« ramasser l'objet 42 ») appliquée par une seule fonction ; les entrées clavier ne modifient jamais l'état directement. En réseau, on enverra ces commandes.
  3. La logique de jeu avance par pas fixes, séparés de la fréquence d'affichage.
  4. L'état du jeu (ce qui doit être partagé) est séparé de la présentation (vent, poussières, herbe qui plie, post-traitement : locaux à chaque joueur).
- Le monde se partage par sa graine (génération déterministe) ; seules les modifications circulent.
- Points durs connus : l'autorité sur les modifications du monde et la synchronisation des anomalies (envoyer leur état, ou compter sur un déterminisme exact entre machines, fragile en flottants et sur GPU). À trancher le moment venu.
- À appliquer en premier lors du ramassage et de l'inventaire.

## 2026-10-05 — Cycle jour et nuit
- Journée de 20 minutes réelles (`game/src/clock.rs`), départ à 16 h 30 ; touche T maintenue : accéléré ×60. Option de capture `--hour H`.
- `render/src/sky.rs` : moments clés écrits à la main (nuit, aube, matin, midi, heure dorée, coucher, heure bleue), interpolés ; soleil de 5 h 30 à 20 h, lune la nuit ; la lumière s'éteint près de l'horizon, donc le passage soleil ↔ lune est invisible. L'étalonnage du post-traitement (teintes, exposition, saturation réduite la nuit) suit l'heure.
- L'eau reflète le ciel (Fresnel) et, la nuit, les étoiles (grille procédurale sur la voûte, scintillement).
- La poussière de premier plan s'éteint la nuit (corrigé ensuite).

## 2026-10-05 — Principe directeur du gameplay
- **Actée** : « la nature est plus forte que le joueur ». Tout est touchable, rien n'est contrôlable : des propriétés et des verbes généraux plutôt que des actions codées par objet ; creusage coûteux, borné, avec conservation de la matière et réaction de la nature (eau, éboulement, comblement). Détail dans `design.md`. Remplace l'idée de destruction de blocs libre façon Minecraft et tranche la question de la difficulté.
- Reste ouvert : le verbe central (ce que le joueur cherche au fond).

## 2026-10-05 — Troncs et ciel nocturne
- Troncs ronds qui s'affinent, évasés en racines au pied, écorce par espèce (sillons, plaques, marques de bouleau, anneaux, fissures) ; nouveau matériau `BarkDark` (42).
- Étoiles reflétées : plus grosses (cœur et halo, couleurs variées), reflétées par une surface immobile (les vaguelettes les brisaient en arcs), Voie lactée et lune. La poussière de premier plan s'éteint la nuit.

## 2026-10-06 — Son procédural
- Nouvelle dépendance **`cpal` 0.18** (sortie audio), seule ajoutée à la pile épinglée. Tout le son est synthétisé : aucun fichier audio.
- `game/src/sound.rs` (synthèse pure, testée sans carte son) : vent (bruit filtré suivant les rafales, gardé discret), feuillage, rivière et bulles, oiseaux (chœur de l'aube), grillons la nuit, pluie, pas selon le sol. `audio.rs` relie à cpal ; jeu ↔ fil audio par atomiques seulement (pas de verrou ni d'allocation dans le rappel). `listen.rs` mesure ce qu'on entend autour du joueur.
- Sans périphérique audio, le jeu tourne en silence.

## 2026-10-06 — Faune, traces, météo
- Faune (`fauna.rs`) : oiseaux qui s'envolent à l'approche, papillons, poissons qui fuient le joueur dans l'eau, lucioles la nuit (particules à lumière propre : éclat négatif). Animaux vivant dans un rayon autour du joueur et renaissant plus loin ; plus gros que nature pour rester lisibles vus de haut. Tous en pièces articulées instanciées.
- Traces (`traces.rs`) : éclaboussures et ronds dans l'eau, empreintes dans le sable et la neige qui s'effacent, poussière en courant.
- Météo (`weather.rs`) : averses aléatoires (chance constante par seconde, quelques minutes), sol mouillé qui sèche, brume de l'aube plus ou moins épaisse selon le jour ; gouttes en traînées, ciel assombri et grisé. Touche R : pluie forcée ; capture `--weather rain`.

## 2026-10-06 — Artisanat par propriétés
- **Actée** : pas de recettes. Un objet = matériaux + forme ; ses capacités sont calculées à partir des propriétés (tranchant, levier, solidité du maillon faible…). Verbes généraux (assembler, tailler, tresser, chauffer, mouiller, sécher), qualité selon le matériau et son état (vert, sec, mort), usure et retour à la nature, pas d'escalade de puissance, ressources locales au biome. Détail dans `design.md`. Conséquence technique : les objets du jeu portent des propriétés numériques, pas un type fixe.

## 2026-10-06 — Ondes, empreintes et son retravaillés
- Les ronds de cubes et les empreintes en blocs sont remplacés par des marques dessinées dans les shaders (`render/src/marks.rs`, groupe 0 binding 5) : de vrais paquets d'ondes qui inclinent la surface de l'eau (amplitude en 1/√d, comme une onde circulaire) et des empreintes de semelle (talon et avant du pied) creusées dans la couleur du sable et de la neige, plus grandes que nature pour rester lisibles.
- Son : stéréo (bruits indépendants à gauche et à droite), vent en bruit brun plus doux et plus bas, réverbération (Freeverb réduit) pour oiseaux, grillons et pas, oiseaux avec harmoniques, pas en deux temps (talon, pointe). `--sound-demo DOSSIER` écrit un WAV par ambiance pour écouter chaque couche séparément. Option de capture `--start X,Z` pour placer le naturaliste.

## 2026-10-06 — Fondations du joueur : commandes, objets, survie, interface
- **État partagé** (`game/src/state.rs`) : joueurs (liste avec identifiants), sacs, besoins, plantes retirées du monde, obstacles. Il ne change que par des `Command` (diriger, ramasser, manger, boire) et par pas fixes de 1/60 s (`STEP`, accumulateur dans `main.rs`) ; il raconte ce qui arrive par des `Event`, auxquels la présentation réagit (cacher la plante, son, message). Le visuel pur (vent, particules, herbe couchée) reste hors de l'état. Applique la contrainte multijoueur.
- **Objets par propriétés** (`items.rs`) : une matière (caillou, brindilles mortes, brins d'herbe, fronde, fleur, champignon) dont les capacités se calculent (masse, dureté, tranchant, souplesse, fragilité, inflammabilité, nutrition, toxicité). Toxicités réelles : bouton d'or toxique, coquelicot un peu, marguerite comestible ; amanite tachetée toxique. Les noms décrivent l'apparence, jamais l'effet. Un rocher ne se soulève pas mais donne trois cailloux. Sac : 8 sortes, 6 kg.
- **Survie douce** (`needs.rs`) : faim (30 min réelles), soif (18 min, plus vite à la chaleur, en courant, malade), chaleur (perdue dans le froid et l'eau, regagnée à l'air doux), malaise après un aliment toxique. Température ressentie selon biome, altitude, heure, pluie. Personne ne meurt : on ralentit (jusqu'à 60 %) et l'image s'assombrit sur les bords et se ternit.
- **Interface** (`render/src/ui.rs`, `game/src/hud.rs`) : police pixel 5 × 9 définie dans le code (accents français dessinés par-dessus), passe d'interface par-dessus l'image finale. Sac, actions possibles, trois jauges, heure, messages.
- Touches : E ramasser, 1–8 choisir, F manger, B boire. Capture : `--pick N`.

## 2026-10-06 — Prototype de la boucle originale : l'anomalie-tapis
- Constat : le jeu n'avait que des briques génériques (survie, sac, ambiance empruntée). Son identité tient aux structures dissipatives : on construit d'abord un prototype de la boucle « observer → agir sur un flux → voir basculer → noter ».
- Idée directrice (à éprouver) : tout ce qui vit est une structure dissipative, le naturaliste compris ; une seule grammaire des flux pour tout le monde.
- Anomalie (`game/src/anomaly.rs`) : un tapis Gray-Scott à une couche (64 × 64 cellules sur 16 × 16 cases) dans une clairière proche du départ, alimenté par une source au bord le plus proche de l'eau : F décroît avec la distance (0,046 → 0,012, k = 0,058), pour que plusieurs régimes vivent côte à côte, un diagramme de phase qu'on parcourt. Dessin : micro-cubes violets qui sortent du sol et pâlissent avec V, lumière propre (forte la nuit). Fait partie de l'état partagé.
- Geste : poser un caillou du sac sur le tapis (touche P) : un disque de cellules inertes, des murs que rien ne traverse. Cinq cailloux par rocher.
- Partage du travail : l'alimentation par cellule et les cellules inertes dans `GrayScott::step` sont un exercice pour l'utilisateur (`crates/sim`, tests `#[ignore]` prêts). D'ici là, tout le tapis tourne au régime labyrinthe (F = 0,029).
- Constat de mesure : en 2D, le régime de la source seule (0,046 ; 0,058) est sous la ligne selle-nœud et donne un tapis uniforme sans motif (ajouté à la fiche Gray-Scott, avec les régimes en 2D).
- Correction : le site exigeait une clairière plate, herbeuse et près de l'eau ; le monde par défaut (1) n'en avait pas, l'anomalie n'apparaissait pas. Le site est désormais choisi par une note (dénivelé jusqu'à 8 marches, herbe, eau proche, distance) parmi les carrés secs et sans arbre, devant la caméra de départ et à quelques cases : visible dès l'arrivée dans tous les mondes testés (1 à 8). Le tapis épouse le relief. Sans eau proche, la source jaillit au bord tourné vers le départ.

## 2026-10-06 — Authenticité et laboratoire (nouveau cœur du jeu)
- **Actée (décision de l'utilisateur)** : critère d'authenticité, tout ce que le joueur voit existe dans la réalité. Les anomalies inventées sont abandonnées ; les structures dissipatives reviennent dans la verrerie : réaction de Belooussov-Jabotinski, anneaux de Liesegang, cristallisation, bandes de Turing dans un gel.
- Boucle centrale : prélever dans le monde → analyser au labo → comprendre → retourner dehors avec une hypothèse. Survie, sac et ramassage servent à rapporter des échantillons ; Gray-Scott (et d'autres règles) anime les récipients ; le labo est la progression (verre, creuset, filtre, balance).
- Réconciliation avec « la nature est plus forte » : le labo est une petite île de contrôle ; les réactions sont sensibles (température, concentration, impuretés) ; les matières varient selon le lieu.
- Le tapis d'anomalie dans la clairière est un prototype abandonné ; son code sera retiré ou réutilisé pour les récipients.

## 2026-10-06 — Le joueur construit tout, en commençant par son labo
- **Actée** : voie stricte pour les réactifs (tout vient du monde par des procédés historiques réels, chaîne dans `docs/chimie/belooussov-jabotinski.md`) ; Belooussov-Jabotinski comme première expérience visée (au manganèse puis au cérium : la ferroïne, molécule de synthèse, est hors d'atteinte). Le joueur construit le labo de départ lui-même et l'améliore librement.
- Construction par propriétés (`game/src/build.rs`) : un projet demande des choses ayant les bonnes propriétés, pas des objets nommés. Foyer (6 pierres dures et résistantes à la chaleur), paillasse (10), coupelle (2 matières à modeler). Les structures font partie de l'état partagé ; commandes `Build` et `Use` (action selon le contexte : ajouter du combustible, allumer par friction, cuire, sortir, poser sur la paillasse, recueillir les cendres).
- Le feu : brûle son combustible (0,01 kg/s), s'éteint sans combustible ou sous forte pluie, laisse des cendres (4 %), réchauffe à 3 cases (+14 °C), cuit l'argile en 90 s de feu vif.
- Prélever : sans plante à cueillir, E prend une poignée du sol : argile sur les berges et en savane, sable sur les plages et au désert.
- L'Oregonator est préparé dans `crates/sim` (fiche `docs/reactions/oregonator.md`, tests en exercice) ; la vue de près de la boîte viendra quand une réaction aura lieu dans une coupelle.
- Touches : C construire (puis 1–3), G utiliser. Capture : `--demo-camp`.

## 2026-10-06 — L'artisanat par la physique (remplace la construction par menu)
- **Actée** : « la fonction naît de la physique » et « la matière garde son histoire ». Plus de menu ni de structures prédéfinies (foyer, paillasse retirés) : on pose des objets (P), une simulation de chaleur et de combustion décide (`sim::thermal`, fiche `docs/reactions/chaleur-combustion.md`), écrite par Claude à la demande de l'utilisateur, qui relit.
- Réseau thermique : un corps par objet (constantes localisées), échanges par contact, rayonnement exact en T⁴, panache convectif limité à la température des gaz, enfermement par angles solides, combustion avec part retenue (flamme 5 %, braise 60 %) et part chauffée (couche fine), palier d'ébullition, séchage accéléré pour le jeu.
- Objets posés (`game/src/objects.rs`) avec histoire ; transformations : coupelle crue → terre cuite (> 600 °C pendant 60 s), coupelle humide → tessons (la vapeur la fait éclater), combustible brûlé → cendres. Argile de berge ou rouge (savane) : l'origine suit la matière.
- Gestes : poser (P), reprendre (E, sauf si trop chaud), modeler (F, argile), frotter (G maintenu, 8 s : une braise sur le combustible sec le plus proche des mains). Poser couche l'herbe autour.
- Calibré sur des grandeurs réelles et vérifié par des traces : un nid de deux poignées d'herbe prend en ~60 s à la braise, les brindilles posées dessus en ~30 s, ~45 kW, ~1100 K ; une coupelle dans un feu de six fagots monte à ~840 °C (plage réelle de la terre cuite). Les tests encodent la bonne procédure (amadou d'abord) et l'échec de la mauvaise.
- Capture : `--demo-fire` refait ces gestes.

## 2026-10-06 — Feu jouable, souris, gestes
- Le feu était infaisable sans connaître la technique. Ajouts réalistes : **souffler** (G maintenu quand quelque chose rougeoit aux mains) apporte de l'air même à un corps enfermé et accélère la combustion (×3) ; la braise tombe dans l'**amadou** (le combustible sec qui s'allume le plus bas) ; un combustible chaud **fume** avant de s'enflammer (pyrolyse), signe qu'il va prendre. Test : en soufflant, même une touffe étouffée sous des brindilles finit par les allumer.
- Souris : un rayon depuis le curseur jusqu'au sol (à travers feuillages et plantes) ; clic gauche pose, clic droit reprend ou cueille, glisser avec le bouton droit tourne la caméra ; une petite marque montre où l'on pointe (lumineuse si l'on peut poser), portée du bras 2,2 cases. Commandes `LayAt` et `PickAt`.
- Gestes animés (`naturalist::GestureKind`) : se pencher pour poser ou prendre, à genoux au foret (archet qui va et vient), penché pour souffler, main à la bouche, à genoux pour boire, pétrir l'argile. Joués à partir des événements, ou tant que G est maintenu. Capture : `--pose`.

## 2026-10-06 — Propagation du feu, creux dans le sol
- Le feu ne passait d'un objet à l'autre qu'au contact (tout ou rien : 1,5 s à 4 cm d'écart, jamais à 8 cm). Le panache a désormais un volume : il décroît sur ~12 cm d'écart (jusqu'à 45 cm) et favorise le haut (×3 au-dessus, ×0,5 en dessous). Mesuré entre deux fagots : 1,5 s au contact, 6 s à 40 cm entre centres, 15 s à 50 cm.
- Hystérésis de la flamme : une flamme allumée s'entretient jusqu'à 60 K sous son seuil et brûle au moins à mi-régime (ses gaz chauffent le solide). Sans elle, une petite flamme qui chauffait ses voisins s'éteignait d'elle-même.
- Prélever le sol laisse un creux (plus profond si l'on creuse au même endroit), visible, devant soi (80 cm) ou à l'endroit visé par la souris (clic droit). Le creux fait partie de l'état partagé.

## 2026-10-06 — Sorts et verbe central (décisions de l'utilisateur)
- **Actée** : de vrais sorts. Observer assez longtemps un comportement anormal (animaux d'abord, puis plantes, arbres, rivières) donne un sort lié à cette anomalie. **Conséquence** : le critère « tout ce que le joueur voit existe vraiment » est abandonné ; il devient « le monde physique obéit à de vraies lois, le surnaturel est rare, cohérent et se mérite par l'observation ». Claude avait recommandé des savoirs réels à la place (prévoir la pluie, trouver l'eau, lire le sol) ; l'utilisateur a choisi les sorts.
- **Actée** : verbe central, le joueur est la **cause sans le vouloir** de la disparition : prélever et expérimenter altèrent le monde, comprendre a un coût.
- Noyau retenu (`design.md`) : comprendre un monde fragile en reproduisant ses structures dans une verrerie ; boucle prélever → labo → déduire → dehors ; l'écologie donne la raison d'avancer.
- À trancher : la nature des sorts et leur coût (suggestion : qu'ils obéissent à la grammaire des flux).

## 2026-10-06 — Outils par la matière
- Une seule commande de travail des mains (F, `Command::Work`), qui fait ce que permet ce qu'on tient : modeler une coupelle (2 argiles), **tailler** (frapper un caillou avec un autre : roche sombre à grain fin, éclat tranchant 70 % ; caillou clair grenu, 15 %, sinon débris), tirer 3 baguettes d'un fagot, **emmancher** (éclat + baguette + fibre souple → couteau).
- Le couteau coupe les buissons (bois vert, plein de sève : il doit sécher avant de brûler) ; il s'use à chaque coupe (8), puis la ligature lâche et rend l'éclat et la baguette. Un éclat tenu à nu se brise souvent (40 %).
- Hasard tiré d'un générateur à graine (déterministe). Exemple de rituel des sorts consigné dans `design.md` (cerfs en cercle les soirs de pleine lune).

## 2026-10-06 — Sac, icônes, creuser pour de vrai
- Sac ouvrable (Tab ou I) : 8 cases avec icônes, détail de l'objet choisi (masse et propriétés en mots : « très dur », « tranchant »…), clic pour choisir, X jeter un exemplaire, Maj+X toute la pile. Icônes tirées des modèles voxels (vue de dessus ou de face, celle qui montre le plus) : toujours procédurales.
- Creuser modifie le monde : 3 poignées au même endroit retirent le cube du dessus (`World::remove_top`, événement `Excavated`), et la couche du dessous apparaît. Un cube faisant 1 m, c'est une simplification d'échelle ; creuser finement demandera les micro-briques. Le sol entier est remaillé à chaque fois (quelques dizaines de ms) : à remplacer par un remaillage par tronçon.
- Limite connue : l'eau ne s'écoule pas encore dans un trou creusé près d'elle.

## 2026-10-06 — Micro-cubes, remaillage par tronçon, eau dans les trous
- Remplace « 3 poignées enlèvent un cube » : une cellule creusée se subdivise en 4 × 4 × 4 micro-cubes (25 cm), stockés seulement pour les cellules touchées (`World::dig`, `micro`). Chaque poignée retire le micro-cube le plus haut sous les mains : un creux se forme. Cellule vidée → la colonne descend d'un cran.
- Le joueur heurte les micro-cubes restants (marches de 25 cm), on pose les objets sur la vraie surface (`surface_height`).
- Le sol est maillé par tronçons de 32 × 32 colonnes (parties d'un même volume dans le renderer) ; creuser ne remaille que le tronçon touché (et son voisin si la cellule est au bord) : ~0,7 ms. Les micro-cubes sont ajoutés au maillage du tronçon (matériau dans le sommet).
- Eau : quand une cellule est vidée à côté d'une eau plus haute que son fond, l'eau remplit le trou et coule dans les trous voisins plus bas (remplissage), puis la surface de l'eau est remaillée. Pas encore d'écoulement dynamique.

## 2026-10-06 — Écologie vivante (première version)
- `game/src/ecology.rs`, écrit par Claude à la demande de l'utilisateur. Le tapis végétal et les buissons vivent : habitat H (biome, humidité selon le climat et la distance à l'eau, lumière selon l'ombre des houppiers, sol), compétition (encombrement c des voisines dans 0,9 case, capacité K = H / (1 + 1,2 c)), croissance logistique ds/dt = r s (1 − s/K), mort (taille < 0,08 ou vieillesse), graines des plantes adultes selon l'espèce (herbe : beaucoup, près ; fougère : spores lointaines, ombre humide ; champignons : ombre humide seulement), germination avec probabilité H là où il y a de la place. Un pas toutes les 2 s réelles (journée de 20 min), suit l'accélération du temps.
- Le joueur est la cause : cueillir, creuser (les plantes partent avec la terre) et le feu (il tue les plantes à 45 cm de ce qui brûle) dégagent le sol ; il est recolonisé, l'herbe d'abord (testé).
- L'état du jeu possède sa liste de plantes (celles du monde, puis les pousses) ; la scène ajoute et redimensionne les instances. Population bornée à 2 × l'initiale.
- Observé sur 6 jours : la prairie se densifie, les touffes posées sur la roche meurent, les fleurs reculent face à l'herbe. À régler : les fleurs disparaissent peut-être trop ; arbres pas encore vivants ; nouvelles pousses non couchées au passage ; le feu ne se propage pas encore à l'herbe sèche (elle meurt sans brûler).
- Capture : `--fast` (temps × 60 pendant `--time`).

## 2026-10-06 — Écologie : Lotka-Volterra, arbres vivants, incendies
- Compétition de Lotka-Volterra à plusieurs espèces : ds_i/dt = r s_i (1 − (s_i + Σ α_ij w_ij s_j)/H_i), α = 1 dans une espèce, moins entre espèces (herbes 0,4 ; herbe–arbuste 0,25 ; arbres 0,7 ; arbres sur herbes 0, ils agissent par l'ombre ; herbes sur jeunes arbres 0,15). Comme chaque espèce se gêne plus elle-même que les autres, elles coexistent (testé : les fleurs ne sont plus éliminées par l'herbe).
- Trois strates (herbe, arbuste, arbre), chacune avec sa portée de compétition, sa place minimale, sa portée de feu.
- Les arbres vivent : graines, jeunes plants (dessinés minuscules), croissance lente, mort. L'ombre n'est plus figée : elle est recalculée à chaque pas depuis les houppiers vivants, donc un arbre qui pousse assombrit le sol et un arbre mort rend la lumière (testé). Pionniers héliophiles (bouleau, pin) contre feuillu tolérant à l'ombre : succession. Un jeune arbre assez grand s'inscrit dans la grille du monde (tronc solide) ; un arbre mort en est effacé (`World::stamp_plant` / `unstamp_plant`, empreinte commune `vegetation::footprint`).
- Incendies : un feu posé par le joueur allume les plantes qui le touchent ; une plante en feu allume ses voisines avec une probabilité par seconde selon leur inflammabilité, leur sécheresse, la distance et le vent ; la pluie l'étouffe ; une plante consumée meurt. Une touffe représente une portion de prairie : le feu passe à la suivante (1,7 case pour les herbes). Mis à jour à chaque image.
- Les nouvelles pousses se couchent au passage.
- Coût mesuré : un pas de vie ≈ 3,3 ms pour 8 000 plantes, toutes les 2 s réelles (30 pas/s en accéléré).
- À peaufiner (prévu) : paramètres des espèces, saisons, banque de graines, herbivores, cendres qui enrichissent le sol, arbres qui tombent.

## 2026-10-06 — Enquêter : anomalies, sorts, carnet, animaux réels
- **Actées (discussion avec l'utilisateur)** : le verbe central est **enquêter**. La nature est la source de la magie, pratiquée par les êtres vivants. Une anomalie (lieu, date, lune) se gagne en la **comprenant** (indices, puis se placer au bon endroit, au bon moment, avec le bon vent), pas en attendant. Observer = se cacher (vent, bruit, lumière, couvert). Plus tard : provoquer pour comprendre (coût écologique), anomalies croisées, vieil homme. Sorts variés dans un arbre caché ; logique fixe, incarnation générée. Carnet lumineux au départ, qui se remplit seul (hypothèse : le joueur relie les pages ; marques peut-être plus tard). Animaux réalistes hors anomalies. Monde infini plus tard (option 3 : le cœur du jeu d'abord).
- Plan : carnet → lune, calendrier, vent changeant → perception animale et cerfs réalistes → indices et rituel → premier sort.

## 2026-10-06 — Lune, vent changeant, cerfs, carnet, premier sort
- **Calendrier et lune** : jours numérotés, cycle lunaire stylisé de 8 jours de jeu (29,5 jours réels feraient dix heures de jeu entre deux pleines lunes), pleine lune la 3e nuit. La lumière de la lune et son reflet suivent la part éclairée du disque, (1 − cos 2πφ)/2.
- **Vent** : sa direction tourne au fil des heures (une dérive sur quelques jours et une composante quotidienne), fonction pure du temps (`game/wind.rs`), transmise au shader par deux composantes libres du tampon d'atmosphère. Les plantes, feuilles, la pluie, le feu et l'odorat des cerfs le suivent ; les nuages gardent leur dérive.
- **`Conditions`** : le moment (heure, jour, lune, pluie, vent) passe d'un bloc à `GameState::step`.
- **Cerfs** (`game/deer.rs`, dans l'état du jeu) : comportement réaliste hors anomalie (voir design), distances comprimées (une case = 1 m, monde de 256 m). Soupçon par cerf qui monte avec la vue, l'ouïe, l'odorat, un feu la nuit ; seuils : vigilance, alarme, fuite de toute la harde ; méfiance 3,6 h après une fuite. Le rituel de pleine lune et ses présages ; l'anneau de terre foulée est creusé dans le monde à la génération (`World::wear`). La prairie : la plus dégagée possible (8, puis 6, puis 4 cases sans arbre), plate, à 30-90 cases du départ.
- **Accroupi** (C) : plus lent, plus bas, plus discret.
- **Carnet** : posé brillant devant le joueur au départ ; se remplit seul, une page par genre d'observation (jour, heure, lune, vent, lieu compté en pas depuis l'endroit où il était), quelques mots rares. **Les croquis sont faits à partir de ce qui était vraiment à l'écran** : l'image HDR de la scène est relue (`Renderer::snapshot`) puis passée en encre (contours de Sobel sur la luminance floutée, seuil adaptatif, hachures dans les ombres, bords estompés).
- **Sorts** : comprendre est séparé du carnet (le naturaliste comprend, le carnet note). Premier sort : Forme du cerf (V), qui bascule la forme ; pas de mains sous cette forme. Coût, durée et autres sorts : encore ouverts (pistes dans le design).

## 2026-10-06 — Corrections après le premier essai du rituel
- **Bug** : la harde vivait en temps réel même quand le jour était accéléré (T) ; la nuit de pleine lune passait en quelques secondes, le cercle n'avait pas le temps de se former. Elle vit maintenant en temps de jeu (un pas de harde par unité d'accélération), et la compréhension se compte aussi en temps de jeu. Testé dans les deux cas.
- **Retour visible** : pendant qu'on comprend le rite, des poussières argentées quittent le cercle et viennent au naturaliste, de plus en plus nombreuses ; le message « Vous avez compris » reste 8 s.
- **Vent lisible** : une flèche sous l'heure, orientée selon la caméra, montre où le vent emporte l'odeur.
- **Cerfs** : collisions avec les troncs ; modèle refait en formes continues (voxels de 5 cm, tronc elliptique, cou oblique, tête effilée, grandes oreilles, dos plus sombre, ventre et croupe pâles), aux proportions d'une biche.

## 2026-10-06 — Incantation d'une minute, cerfs articulés
- **Le rituel dure une minute de jeu** (22 h à 23 h 12, après un rassemblement dès 21 h 20) et devient une chorégraphie d'incantation : immobiles face au centre, saluts synchronisés et coup de sabot qui fait le tour du cercle ; procession ; têtes levées vers la lune (fils de lumière vers le centre, colonne vers le ciel) ; éclat final. Le sort s'apprend à ce sommet, après au moins 20 s d'observation sans être repéré.
- **Cerfs articulés** : pattes en deux segments (genou, jarret), allures réelles (pas à quatre temps, trot diagonal, bonds au galop avec le dos qui ondule), cou et tête séparés, queue qui se relève à l'alarme. La Forme du cerf est un cerf mâle à bois.
- Correction : un cerf qui arrive sur sa place ne tourne plus en orbite autour (il ralentit, et tourne avant d'avancer).

## 2026-10-06 — L'écosystème comme lieu des bifurcations ; la faune dedans
- **Sol** (`game/soil.rs`) : variables lentes par parcelles de 8 × 8 cases, eau W et matière organique N, d'après Rietkerk (infiltration qui croît avec la couverture, seuil net en eau, suintement latéral par un laplacien en double tampon). Ce que le sol offre multiplie l'habitat des plantes, rapporté à l'état de départ (le monde généré est à l'équilibre). Testé : bistabilité et hystérésis (champ moyen) ; une tache nue se referme par ses bords, une grande étendue reste nue (avec les vraies plantes).
- **Pâturage** : les cerfs mangent vraiment les plantes autour d'eux (Holling II, appétence par espèce, jeunes arbres compris). Ils ont des réserves, mettent bas (fécondité qui baisse avec la densité) et meurent de faim. La harde change de pâture quand la sienne est épuisée et revient au cercle pour le rituel. Mesuré sur 40 jours sans joueur : 7 à 8 cerfs, prairie broutée mais vivante.
- Le carnet note un faon né et une biche morte de faim.
- **Rituel raccourci** : rassemblement à 21 h 40, incantation de 22 h à 22 h 36 ; sort compris au sommet, après 10 s d'observation sans être vu.
- Fiche : `docs/reactions/ecologie.md`.

## 2026-10-06 — Direction : la vision B, le druide sur une planète vivante
- **Actées** (discussion avec l'utilisateur) : un druide apprend la magie d'une planète vivante en observant ceux qui la pratiquent, et y vit longtemps (jouer longtemps, comme un monde où l'on vit, plutôt que recommencer souvent). Résumé dans `docs/vision.md`.
- **Transformation** : chaque forme apprise change une part des mots du carnet en signes ; en partie réversible ; le joueur choisit jusqu'où aller.
- **Source** : un grand cycle, une onde lente qui parcourt la planète ; les anomalies en sont des rides ; la grande marée est un rendez-vous qui revient, plus fort quand la planète est saine.
- **Laboratoire** : abandonné comme moteur ; son esprit reste dans les motifs reconnus dans le paysage ; `crates/sim` anime les anomalies.
- **Monde** : une planète finie qui se referme sur elle-même (remplace le monde infini : dans un monde infini, on fuit les conséquences). D'autres planètes peut-être plus tard.
- **Anomalies émergentes** : elles naissent là où un milieu est sain depuis longtemps et meurent quand il se dégrade.
- Feuille de route et catalogues créés : `docs/roadmap.md`, `docs/catalogue/`.

## 2026-10-06 — Chaque animal façonne la terre ; la roue des sorts
- **Actées** : aucun animal n'est un décor ; chacun transporte, retire, ajoute ou transforme quelque chose du monde, et ces actions passent par l'écologie et le sol (écureuils et geais qui plantent les chênes avec leurs caches oubliées, sangliers qui retournent la terre, castors qui inondent…). Table dans `catalogue/animaux.md`.
- **Les sorts se choisissent sur une roue** (maintenir, viser, relâcher), dessinés en signes.

## 2026-10-06 — Les animaux vivent au rythme des saisons
- **Actée** : chaque espèce suit son calendrier réel ; elle ne se reproduit qu'à sa saison (les faons naissent à la fin du printemps après le rut d'automne). Tableau dans `catalogue/animaux.md`, inscrit à l'étape 1 de la feuille de route. Les naissances « toute l'année » de la harde sont provisoires.

## 2026-10-06 — Sauvegarde
- **Format maison, sans dépendance** (`game/save.rs`) : un écrivain et un lecteur binaires, un trait `Persist`, des macros pour les structures et les énumérations simples. En-tête `DSPF`, version, graine : une sauvegarde d'une autre version ou d'un autre monde est ignorée (nouvelle partie). Écrite dans un fichier temporaire puis renommée.
- **Le monde est régénéré depuis sa graine**, puis ce qui a changé est restauré par-dessus : voxels (compressés par plages), hauteurs du sol et de l'eau, cellules creusées, plantes, vies des plantes et sol, harde, objets et leur thermique (les liens sont recalculés), joueurs, carnet et croquis, heure. Ce qui se recalcule (habitat, ombre, couverture, obstacles, ce qui se ramasse) n'est pas enregistré.
- **Testé** : une partie sauvegardée puis rechargée continue exactement comme l'originale (déterminisme). Taille ~2,4 Mo, ~3,5 ms.
- Sauvegarde automatique toutes les 2 minutes réelles et à la fermeture ; `--new` pour recommencer. Les captures ne lisent ni n'écrivent de sauvegarde.

## 2026-10-06 — Les saisons
- **Calendrier** (`game/season.rs`) : une saison = deux lunes (16 jours de jeu, ~5 h de jeu), une année = 64 jours (~21 h). La partie commence à la fin de l'été. Température : les moyennes de l'été moins jusqu'à 18 °C au cœur de l'hiver.
- **Rendu** : un `vec4` « saison » ajouté au tampon d'atmosphère (feuilles qui virent, feuilles tombées, neige, glace) et l'herbe sèche dans `fog_color.w`. Dans le shader, chaque voxel a son nombre aléatoire : une feuille vire puis disparaît d'un bloc (`discard`), la neige couvre par plaques puis en nappe ce qui fait face au ciel, la glace rend l'eau opaque et pâle. Les ombres des arbres nus restent pleines (la passe d'ombre n'a pas d'étage de fragments) : à corriger plus tard.
- **Neige et glace** (`weather.rs`) : sous 1 °C la pluie tombe en flocons ; la couverture monte en ~3 h de chute et fond selon le dégel ; l'eau gèle après ~10 h sous −1 °C.
- **Plantes** : croissance rapide au printemps, presque nulle l'hiver ; les herbes perdent leurs parties aériennes l'hiver mais gardent leurs racines (elles comptent pour la couverture du sol, et ne se broutent pas sous un plancher pendant la dormance) ; herbes vivaces (l'herbe vit ~2 ans de jeu, la fougère plus) ; **banque de graines** : les graines d'automne et d'hiver attendent le printemps ; l'évaporation suit la chaleur. Le plafond de plantes (coût) ne s'applique plus sur sol nu, qui reste recolonisable.
- **Corrections trouvées en simulant une année** : sans ces réglages, l'hiver faisait basculer la prairie en désert et tuait la harde ; et l'irréversibilité du surpâturage venait en partie du plafond de plantes. Le suintement latéral de l'eau est ramené à 0,1/jour : une tache nue d'une parcelle se referme, une étendue de plusieurs parcelles reste nue (testé avec les vraies plantes).
- **Cerfs** : rut et brame à l'automne (un cerf rejoint la harde), conception pendant le rut, naissances à la fin du printemps (un faon par biche au plus), faons adultes en un an, métabolisme ralenti l'hiver (−40 %, comme le cerf réel), réserves qui tiennent ~2 semaines de jeûne, bois tombés à la fin de l'hiver (nouvel objet, à ramasser). Mesuré sur une année : la harde passe l'hiver amaigrie, 3 faons au printemps, prairie vivante.
- Sauvegarde : version 3 (année dans le moment, banque de graines, cerfs gestants).

## 2026-10-06 — Écureuils, chênes et noisetiers
- **Chêne et noisetier** ajoutés au monde (forêts, un peu les prairies). Le chêne a des glands lourds qui tombent sous lui : il ne se répand loin que par les animaux.
- **Écureuils** (`game/squirrels.rs`, dans l'état du jeu) : 8 écureuils vivent autour des chênes les plus proches du départ. Comportement réel : diurnes, enterrent les noix une par une à 4–18 cases (dispersion), retrouvent leurs caches par la mémoire (85 %) ou l'odorat (1,5 case, y compris celles des autres), mangent trois noix par jour dans les mois maigres, fuient vers leur arbre à l'approche. Une récolte par an tirée au hasard (années de glandée). Au début du printemps, les caches restantes passent dans la banque de graines : des chênes et des noisetiers poussent là où les écureuils les ont oubliés.
- Le joueur peut déterrer une cache (« terre remuée » quand il est dessus) : gland (amer, tanins) ou noisette. Le carnet note un écureuil qui enterre, et un qui retrouve.
- Le modèle est légèrement agrandi (×1,4) pour être vu de la caméra haute.
- Sauvegarde : version 4.

## 2026-10-07 — Le sol se voit ; les sons de la harde
- **Sol visible** : toutes les 10 s de jeu, chaque colonne de sol (herbe, litière, herbe sèche, terre) est comparée à sa parcelle. Une parcelle tombée sous 60 % de sa couverture de départ montre de la terre nue, d'autant plus qu'elle est dénudée (choix de colonnes par hachage). Le monde change vraiment (`World::set_surface`, sauvegardé) ; seules les colonnes mises à nu par le jeu reverdissent ensuite (la terre nue d'origine n'est jamais touchée). Côté rendu, seul le champ de matériaux est renvoyé au GPU (pas de remaillage).
- **L'anneau du rituel** suit la harde : foulé tant que le rituel est tenu, repris par l'herbe, cellule après cellule, au cours de la troisième lune sans rituel.
- **Sons** : aboiement (un éclat de bruit filtré et une fondamentale qui tombe), coup de sabot (un choc grave), brame (2,2 s, dents de scie glissant de 160 à 90 Hz, résonance de gorge, grain rauque). Portée : 50, 20 et 100 cases. Écoutables avec `--sound-demo DIR`.
- Sauvegarde : version 5.

## 2026-10-07 — La planète
- **Un tore** : le monde se referme d'est en ouest et du nord au sud. Tout le bruit est périodique (indices du réseau pris modulo le nombre de mailles par tour : `noise::value_tiled`, `fbm_tiled`, `ridged_tiled`) ; il n'y a plus de bande de mer imposée aux bords : la mer est faite des grandes étendues sous le niveau de la mer (au moins 1 % du monde), les plus petites deviennent des lacs ; les voisinages (distances, inondation prioritaire des rivières, creusement des lits) passent par-dessus la couture. **Climat par la latitude** : froid vers z = 0 (les pôles), chaud à mi-chemin (l'équateur).
- **Taille** : 512 × 64 × 512 (`WorldConfig::standard`). Les tests utilisent `WorldConfig::small` (256), quatre fois moins cher. Mesures : génération ~0,8 s, ~825 Mo de mémoire.
- **Coordonnées** : les accès au monde (`block`, `ground_top`, `water_level`, `biome`, `micro`, `surface_height`) ramènent leurs coordonnées dans le monde ; `World::wrap`, `column` et `nearest` (la copie la plus proche d'un point) servent aux déplacements (naturaliste, cerfs, écureuils), aux sens des cerfs, au voisinage des plantes, aux graines, au suintement du sol, au rayon du curseur.
- **Rendu** : chaque chose est dessinée à sa copie la plus proche du point regardé (pas de l'œil, qui peut être loin en vue plongeante), le décalage étant pris d'un ancrage commun à tous ses sommets (la cellule d'une face, le pied d'une plante, l'origine d'une pièce, le centre d'une particule) : rien ne se déchire à une demi-planète. La taille du monde et le point regardé passent dans le tampon de la caméra (`world`, à l'offset 160). Les ombres couvrent ±72 cases autour du regard.
- **Coût** : n'est dessiné que ce qui est à moins de 110 cases (plantes, choisies de nouveau tous les 8 cases) ou 150 cases (morceaux de relief, par leur boîte) du regard. 11 → ~54 images/s ; le jeu est alors limité par la carte graphique. L'écologie fait des pas plus longs (jusqu'à 30 s de jeu) quand le jour est accéléré : même coût par seconde réelle, toujours stable.
- **Limites** : les traces (empreintes, ondes) ne passent pas la couture ; des palmiers peuvent pousser sur les plages froides (la plage ne dépend pas du climat).
- Sauvegarde : version 6 (les anciennes, d'un monde de 256, sont ignorées).

## 2026-10-07 — Écureuils visibles ; une harde sur chaque planète
- Sur la planète de 512, aucune prairie ne remplissait les critères de la harde : pas de cerfs. La recherche se fait maintenant par paliers (plus dégagée et plate d'abord, puis moins, et plus loin, jusqu'à 220 cases). Vérifié sur 6 graines : une harde à chaque fois.
- Écureuils : dérangés, ils grimpent sur le tronc et s'y plaquent, tête en bas, visibles (seule la nuit les cache, dans leur nid) ; actifs de 7 h à 19 h 30 ; fuient à 4 cases, redescendent après 10 à 20 s ; un défaut les faisait à peine bouger (ils n'avançaient qu'à l'instant d'une décision) ; modèle agrandi (×1,8 la taille réelle) pour la caméra haute.

## 2026-10-07 — La caméra
- **Un seul zoom** (`game/src/camera_rig.rs`) règle la distance, l'inclinaison et le champ de vision : de la vue plongeante (110 cases, 58°, champ de 36°) jusqu'à hauteur d'homme (4,5 cases derrière le naturaliste, 6°, champ de 58°, regard au-dessus de sa tête). Au départ : ~40° comme avant, même cadrage (champ plus large, caméra plus proche). Glisser avec le clic droit tourne la vue (en douceur) et l'incline jusqu'à ±40° autour de l'inclinaison du zoom. Pas de touche pour pivoter : la souris suffit (choix du joueur). `--zoom F` multiplie toujours la distance par défaut, mais l'inclinaison suit.
- **Ciel** : un fond dessiné selon la direction de chaque pixel (premier dessin de la passe, sans profondeur) : la brume à l'horizon, la couleur du ciel au zénith, un halo autour du soleil, les étoiles et la lune la nuit (plus faibles qu'en reflet, et près de l'horizon). Il n'existait pas : la vue plongeante ne voyait jamais l'horizon.
- **Rayon de dessin selon le zoom** (`OrbitCamera::reach`) : le relief et les plantes sont dessinés jusqu'où les coins hauts de l'écran rencontrent le sol (avec une marge), entre 64 et 160 cases, et le monde se fond entièrement dans la brume avant ce bord (plus de coupure nette). Le plan lointain suit.
- **Découpe** : ce qui se dresse entre l'œil et le naturaliste, au-dessus de ses pieds (arbres, collines), n'est pas dessiné dans un cône de l'œil jusqu'à lui (une fenêtre ronde à l'écran, 2,2 cases de rayon chez lui, bord tramé), ni tout près de l'œil (une canopée dont il sort). Le sol plat, ce qui est juste devant lui, les personnages et les ombres ne sont jamais coupés.
- **Relief** : l'œil ne rentre plus dans le sol : un rayon de la cible vers l'œil (pas de 0,1 case, marge de 0,35 case) le ramène à l'instant, puis il ressort doucement. Les plantes ne comptent pas (elles sont découpées).
- **Suivi sans à-coups** : le naturaliste (et donc la caméra) est dessiné entre sa position avant le dernier pas fixe et sa position actuelle, selon l'avance de l'image dans le pas suivant (les pas sont à 60 Hz, les images à la fréquence de l'écran) ; la cible suit par un ressort amorti critique (0,12 s à l'horizontale, 0,3 s à la verticale : un saut ou une marche ne secoue pas la vue).
- Le flou miniature (tilt-shift) s'efface quand la vue descend sous 35° (rien de miniature à hauteur d'homme) ; la parallaxe de la poussière suit le champ de vision.
- Correction : un cube entamé (sable, argile, terre creusés) devenait invisible tout en gardant sa collision depuis la planète. Ses micro-voxels portent leur matériau à la place de leur cellule, et le shader prenait ce code pour la cellule de l'ancrage : ils étaient dessinés à l'autre bout du monde. Ils s'ancrent désormais à leur propre position.
