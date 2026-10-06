//! Simulation pure : grilles, règles de réaction, intégrateurs, métriques.
//! Aucune dépendance GPU ni fenêtre. C'est la référence de vérité du projet.

pub mod gray_scott;
pub mod grid;
pub mod hydrology;
pub mod oregonator;
pub mod rng;
pub mod thermal;
