//! Turns a simulation field into a voxel mesh.
//!
//! A cell is solid when its value exceeds a threshold. Only faces between a solid cell and
//! an empty one (or the outside of the grid) are emitted: faces between two solid cells can
//! never be seen ("culled meshing"). Colour does not matter here, it comes later from the
//! field itself (see docs/decisions.md, "Géométrie par occupation, couleur par texture 3D").
//!
//! Each corner also gets an ambient occlusion value: a corner tucked against solid
//! neighbours receives less light from the sky, so creases and cavities darken.

use sim::grid::Field3;

use crate::mesh::{FACE_CORNERS, FACE_NORMALS, MAX_GRID_SIZE, MeshData, face_tangents};

/// Rebuilds `mesh` from the cells of `field` whose value is strictly above `threshold`.
///
/// Cell (x, y, z) occupies the unit cube [x, x+1] × [y, y+1] × [z, z+1].
/// Outside the grid counts as empty, so the boundary of the parcel is closed.
/// `mesh` is cleared first and its memory reused.
///
/// Panics if a side of the grid exceeds `MAX_GRID_SIZE` cells.
pub fn mesh_field(field: &Field3, threshold: f32, mesh: &mut MeshData) {
    mesh.clear();
    let dims = field.dims;
    assert!(
        [dims.nx, dims.ny, dims.nz]
            .iter()
            .all(|&n| n <= MAX_GRID_SIZE as usize),
        "grid {dims:?} too large to mesh"
    );
    // NaN compares false, so a corrupted cell shows as empty instead of crashing.
    let solid = |x: isize, y: isize, z: isize| -> bool {
        let inside = x >= 0
            && y >= 0
            && z >= 0
            && (x as usize) < dims.nx
            && (y as usize) < dims.ny
            && (z as usize) < dims.nz;
        inside && field.get(x as usize, y as usize, z as usize) > threshold
    };

    // z outermost, x innermost: same order as the memory layout of `Field3`.
    for z in 0..dims.nz as isize {
        for y in 0..dims.ny as isize {
            for x in 0..dims.nx as isize {
                if !solid(x, y, z) {
                    continue;
                }
                for normal in FACE_NORMALS {
                    let cell = [x, y, z];
                    // The cell in front of the face: the face is visible only if it is empty.
                    let front: [isize; 3] = std::array::from_fn(|i| cell[i] + normal[i] as isize);
                    if solid(front[0], front[1], front[2]) {
                        continue;
                    }
                    let (u, v) = face_tangents(normal);
                    let ao = FACE_CORNERS.map(|(su, sv)| {
                        // Towards this corner along each tangent: -1 or +1.
                        let (du, dv) = (2 * su - 1, 2 * sv - 1);
                        let at = |a: isize, b: isize| {
                            let p: [isize; 3] = std::array::from_fn(|i| {
                                front[i] + a * u[i] as isize + b * v[i] as isize
                            });
                            solid(p[0], p[1], p[2])
                        };
                        corner_ao(
                            at(du as isize, 0),
                            at(0, dv as isize),
                            at(du as isize, dv as isize),
                        )
                    });
                    mesh.push_face([x as u32, y as u32, z as u32], normal, ao);
                }
            }
        }
    }
}

/// Ambient occlusion of a face corner from the three cells around it, in the layer in front
/// of the face: the two sharing an edge with the corner (`side_a`, `side_b`) and the diagonal
/// one (`diagonal`). 1 = fully open, 0 = buried.
///
/// With both sides solid the corner is closed off whatever the diagonal holds, hence 0.
fn corner_ao(side_a: bool, side_b: bool, diagonal: bool) -> f32 {
    if side_a && side_b {
        return 0.0;
    }
    let blocked = side_a as u8 + side_b as u8 + diagonal as u8;
    (3 - blocked) as f32 / 3.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim::grid::Dims;

    fn faces(field: &Field3) -> usize {
        let mut mesh = MeshData::default();
        mesh_field(field, 0.5, &mut mesh);
        mesh.face_count()
    }

    #[test]
    fn empty_field_gives_no_face() {
        assert_eq!(faces(&Field3::filled(Dims::cube(4), 0.0)), 0);
    }

    #[test]
    fn single_cell_gives_six_faces() {
        let mut field = Field3::filled(Dims::cube(4), 0.0);
        field.set(1, 2, 3, 1.0);
        assert_eq!(faces(&field), 6);
    }

    #[test]
    fn shared_face_between_neighbours_is_hidden() {
        let mut field = Field3::filled(Dims::cube(4), 0.0);
        field.set(1, 1, 1, 1.0);
        field.set(2, 1, 1, 1.0);
        assert_eq!(faces(&field), 10);
    }

    #[test]
    fn full_grid_only_shows_its_hull() {
        let dims = Dims {
            nx: 3,
            ny: 4,
            nz: 5,
        };
        let hull = 2 * (3 * 4 + 4 * 5 + 3 * 5);
        assert_eq!(faces(&Field3::filled(dims, 1.0)), hull);
    }

    #[test]
    fn threshold_is_strict_and_nan_is_empty() {
        let mut field = Field3::filled(Dims::cube(3), 0.5);
        field.set(0, 0, 0, f32::NAN);
        assert_eq!(faces(&field), 0);
    }

    #[test]
    fn corner_ao_levels() {
        assert_eq!(corner_ao(false, false, false), 1.0);
        assert_eq!(corner_ao(false, false, true), 2.0 / 3.0);
        assert_eq!(corner_ao(true, false, true), 1.0 / 3.0);
        assert_eq!(corner_ao(true, true, false), 0.0);
    }

    #[test]
    fn isolated_cube_is_not_occluded() {
        let mut field = Field3::filled(Dims::cube(3), 0.0);
        field.set(1, 1, 1, 1.0);
        let mut mesh = MeshData::default();
        mesh_field(&field, 0.5, &mut mesh);
        assert!(mesh.vertices.iter().all(|v| v.ao == 1.0));
    }

    #[test]
    fn cube_on_a_floor_darkens_at_the_base() {
        // A 3×1×3 floor at y = 0 with one cube on its centre, at y = 1.
        let mut field = Field3::filled(Dims::cube(3), 0.0);
        for z in 0..3 {
            for x in 0..3 {
                field.set(x, 0, z, 1.0);
            }
        }
        field.set(1, 1, 1, 1.0);
        let mut mesh = MeshData::default();
        mesh_field(&field, 0.5, &mut mesh);

        let cube_sides = mesh
            .vertices
            .iter()
            .filter(|v| v.cell == crate::mesh::pack_cell([1, 1, 1]) && v.normal[1] == 0.0);
        for v in cube_sides {
            // Vertices at the base (y = 1) touch the floor, those at the top (y = 2) do not.
            let expected = if v.position[1] == 1.0 { 1.0 / 3.0 } else { 1.0 };
            assert_eq!(v.ao, expected, "{v:?}");
        }
    }

    #[test]
    fn rebuilding_replaces_previous_mesh() {
        let mut field = Field3::filled(Dims::cube(3), 0.0);
        field.set(1, 1, 1, 1.0);
        let mut mesh = MeshData::default();
        mesh_field(&field, 0.5, &mut mesh);
        mesh_field(&field, 0.5, &mut mesh);
        assert_eq!(mesh.face_count(), 6);
    }
}
