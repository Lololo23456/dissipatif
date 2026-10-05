# Métriques et classement des régimes

## Métriques par simulation

Mesurer sur le champ de l'activateur (v pour Gray-Scott), toutes les 100 étapes, après une période de transitoire (par exemple les 1000 premiers pas ignorés).

| Métrique | Calcul | Ce qu'elle révèle |
|---|---|---|
| Activité | fraction des cellules au-dessus d'un seuil (0,18 pour Gray-Scott) | quantité de structure |
| Moyenne et variance temporelles de l'activité | sur la fenêtre après transitoire | stabilité ou oscillation |
| Entropie spatiale | histogramme de v en 32 cases, entropie de Shannon | homogénéité ou richesse du motif |
| Compressibilité | taille compressée du champ quantifié sur 8 bits divisée par taille brute (lz4 ou zstd) | ordre (compressible) contre bruit (incompressible) |
| Autocorrélation à pas 1 | autocorrélation de la série d'activité | ralentissement critique : proche de 1 près d'une bifurcation |
| Temps de retour | après une petite perturbation, nombre de pas pour revenir à 5 % de l'état initial | ralentissement critique, plus direct mais plus coûteux |

## Règles de classement (point de départ, à ajuster)

1. **Extinction** : activité finale < 0,1 %.
2. **Saturation** : activité > 95 % et entropie spatiale faible (le système est uniforme).
3. **Stable** : variance temporelle de l'activité très faible (écart-type < 1 % de la moyenne).
4. **Oscillant** : variance notable avec un pic net dans le spectre de la série d'activité.
5. **Chaotique** : variance notable sans pic net, et grande sensibilité à la graine.
6. **Motifs** : stable ou faiblement variable, avec compressibilité intermédiaire et entropie spatiale élevée.

Un même point peut être stable avec une graine et autre chose avec une autre : le signaler comme **multistable**.

## Pièges
- Le transitoire peut durer longtemps près des bifurcations : si un point semble « stable » mais que son autocorrélation est proche de 1, il n'a peut-être pas fini d'évoluer.
- Une grille trop petite supprime certains motifs dont la longueur d'onde est grande. Confirmer les points intéressants sur une grille plus grande.
