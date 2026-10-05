//! Grille 3D régulière stockée dans un tableau contigu (x varie le plus vite).

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
