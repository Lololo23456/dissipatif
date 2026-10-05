//! Grilles régulières 2D et 3D stockées dans un tableau contigu (x varie le plus vite).

/// Dimensions d'une grille 3D.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dims {
    pub nx: usize,
    pub ny: usize,
    pub nz: usize,
}

impl Dims {
    pub const fn cube(n: usize) -> Self {
        Self {
            nx: n,
            ny: n,
            nz: n,
        }
    }

    /// Nombre total de cellules.
    pub const fn len(&self) -> usize {
        self.nx * self.ny * self.nz
    }

    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Indice linéaire de la cellule (x, y, z).
    #[inline]
    pub const fn index(&self, x: usize, y: usize, z: usize) -> usize {
        x + self.nx * (y + self.ny * z)
    }
}

/// Champ scalaire sur une grille 3D (une concentration, une température…).
#[derive(Clone, Debug)]
pub struct Field3 {
    pub dims: Dims,
    pub data: Vec<f32>,
}

impl Field3 {
    pub fn filled(dims: Dims, value: f32) -> Self {
        Self {
            dims,
            data: vec![value; dims.len()],
        }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize, z: usize) -> f32 {
        self.data[self.dims.index(x, y, z)]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, z: usize, value: f32) {
        let i = self.dims.index(x, y, z);
        self.data[i] = value;
    }
}

/// Champ scalaire sur une grille horizontale 2D (x, z) : une hauteur de sol, une hauteur d'eau…
/// Même convention que `Field3` : x varie le plus vite.
#[derive(Clone, Debug, PartialEq)]
pub struct Field2 {
    pub nx: usize,
    pub nz: usize,
    pub data: Vec<f32>,
}

impl Field2 {
    pub fn filled(nx: usize, nz: usize, value: f32) -> Self {
        Self {
            nx,
            nz,
            data: vec![value; nx * nz],
        }
    }

    /// Indice linéaire de la cellule (x, z).
    #[inline]
    pub const fn index(&self, x: usize, z: usize) -> usize {
        x + self.nx * z
    }

    #[inline]
    pub fn get(&self, x: usize, z: usize) -> f32 {
        self.data[self.index(x, z)]
    }

    #[inline]
    pub fn set(&mut self, x: usize, z: usize, value: f32) {
        let i = self.index(x, z);
        self.data[i] = value;
    }

    /// Somme de toutes les valeurs, accumulée en f64 pour limiter l'erreur d'arrondi.
    pub fn sum(&self) -> f64 {
        self.data.iter().map(|&v| v as f64).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_is_contiguous_in_x() {
        let d = Dims {
            nx: 4,
            ny: 3,
            nz: 2,
        };
        assert_eq!(d.index(0, 0, 0), 0);
        assert_eq!(d.index(1, 0, 0), 1);
        assert_eq!(d.index(0, 1, 0), 4);
        assert_eq!(d.index(0, 0, 1), 12);
        assert_eq!(d.index(3, 2, 1), d.len() - 1);
    }

    #[test]
    fn set_then_get() {
        let mut f = Field3::filled(Dims::cube(8), 1.0);
        f.set(2, 3, 4, 0.25);
        assert_eq!(f.get(2, 3, 4), 0.25);
        assert_eq!(f.get(0, 0, 0), 1.0);
    }
}
