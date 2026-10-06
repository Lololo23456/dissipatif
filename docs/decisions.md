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
