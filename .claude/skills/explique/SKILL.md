---
name: explique
description: Mode pédagogique pour comprendre un concept, une équation ou un morceau de code du projet en profondeur. À utiliser dès que l'utilisateur demande « explique », « je ne comprends pas », « pourquoi », « comment ça marche », ou pose une question de fond sur la physique hors équilibre, les systèmes dynamiques, la numérique, Rust, wgpu ou WGSL, même s'il ne demande pas explicitement une explication détaillée.
argument-hint: [concept, fichier ou fonction]
---

# Expliquer : $ARGUMENTS

L'utilisateur est étudiant en 3e année d'informatique, passionné par la théorie de la complexité de Prigogine. Il programme bien, il renforce les maths. Le but est qu'il comprenne vraiment, pas qu'il ait une réponse.

## Structure de l'explication
1. **Intuition d'abord**, en une ou deux phrases, avec une image physique si possible.
2. **Le formalisme** : équations ou concepts précis, chaque symbole défini. Pas de saut d'étape.
3. **Le lien avec le projet** : où ça apparaît dans le code ou dans une mécanique de `docs/design.md`. Cite les fichiers et lignes réels, lis-les avant d'en parler.
4. **Un phénomène réel** quand c'est pertinent : bifurcations, ralentissement critique, structures dissipatives, hystérésis.
5. **Une vérification** : termine par une petite question ou un mini-exercice (par exemple « que se passe-t-il si on double dt ? ») et attends sa réponse avant de donner la solution.

## Règles
- Réponds en français.
- Si le concept demande des prérequis qu'il n'a peut-être pas, commence par les vérifier en une question plutôt que de tout supposer.
- Sois honnête sur les limites : si une idée est controversée ou une question ouverte en recherche, dis-le.
- Pour aller plus loin, une seule référence précise vaut mieux qu'une liste (Strogatz pour les systèmes dynamiques, Nicolis et Prigogine pour l'auto-organisation, etc.).
- Ne modifie aucun fichier pendant une explication.
