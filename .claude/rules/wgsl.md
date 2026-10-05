---
paths:
  - "**/*.wgsl"
  - "crates/render/src/**"
---
# Règles WGSL et échanges Rust ↔ GPU

## Alignement mémoire (source n°1 de bugs silencieux)
- Toute struct envoyée au GPU : `#[repr(C)]` et `#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]`.
- En WGSL, `vec3<f32>` est aligné sur 16 octets. Évite `vec3` dans les buffers : utilise `vec4` ou ajoute un champ de remplissage explicite côté Rust.
- Taille d'une struct uniform : multiple de 16 octets.
- Ajoute un test ou une assertion `std::mem::size_of` qui vérifie la taille attendue de chaque struct partagée.
- Garde la struct Rust et la struct WGSL côte à côte dans les commentaires, avec les offsets.

## Compute shaders
- Taille de workgroup ≤ 256 invocations au total (limite par défaut), par exemple `@workgroup_size(4, 4, 4)` ou `(8, 8, 4)` pour les grilles 3D.
- Vérifie les bornes : le nombre de workgroups est arrondi au supérieur, donc certaines invocations tombent hors de la grille.
- Double tampon en deux buffers ou deux textures, jamais lecture et écriture dans la même ressource pendant un pas.
- Pas d'aléa non reproductible : si un shader a besoin de bruit, il prend une graine et un hachage déterministe.

## Validation
- Un hook valide chaque fichier .wgsl avec naga après modification. Si naga n'est pas installé : `cargo install naga-cli --version 30.0.1`.
