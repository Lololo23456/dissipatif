//! Water surface mesh: water is drawn at its exact height, not rounded to voxels, so a stream a
//! few tenths of a cell deep is visible, and the player can later wade into it.
//!
//! Per wet column: a top quad at the water surface, and a side wall towards each neighbour
//! whose level (its water surface, or its ground if dry) is lower, down to that level.

use sim::grid::Field2;

use crate::mesh::{FACE_CORNERS, MeshData};

/// A column is wet when its surface is at least this far above its floor.
const MIN_THICKNESS: f32 = 1e-3;

/// Rebuilds `mesh` with the water surface.
///
/// - `floor`: height of the visible ground top of each column (the top of its voxels).
/// - `surface`: height of the water surface; a column is wet where it is above `floor`.
///
/// Column (x, z) spans [x, x+1] × [z, z+1]. Each vertex gets the cell (x, 0, z), so the water
/// textures are 2D grids stored with a height of 1.
pub fn mesh_water(floor: &Field2, surface: &Field2, mesh: &mut MeshData) {
    assert_eq!((floor.nx, floor.nz), (surface.nx, surface.nz));
    mesh.clear();
    let (nx, nz) = (floor.nx, floor.nz);
    let wet = |x: usize, z: usize| surface.get(x, z) > floor.get(x, z) + MIN_THICKNESS;
    // Height of what a neighbour shows towards us: its water surface, or its ground if dry.
    let level = |x: usize, z: usize| {
        if wet(x, z) {
            surface.get(x, z)
        } else {
            floor.get(x, z)
        }
    };

    for z in 0..nz {
        for x in 0..nx {
            if !wet(x, z) {
                continue;
            }
            let top = surface.get(x, z);
            let own_floor = floor.get(x, z);
            let cell = [x as u32, 0, z as u32];
            let (x0, z0) = (x as f32, z as f32);

            // Top: counter-clockwise seen from above (+y), same corner order as voxel faces.
            let corners = FACE_CORNERS.map(|(su, sv)| [x0 + sv as f32, top, z0 + su as f32]);
            mesh.push_quad(corners, [0.0, 1.0, 0.0], cell);

            // Sides: towards each lower neighbour, from its level (or our floor) up to our top.
            for (dx, dz) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                let neighbour = x
                    .checked_add_signed(dx)
                    .zip(z.checked_add_signed(dz))
                    .filter(|&(xn, zn)| xn < nx && zn < nz);
                // No wall at the edge of the world: the water goes on beyond it.
                let Some((xn, zn)) = neighbour else {
                    continue;
                };
                let bottom = level(xn, zn).max(own_floor);
                if top <= bottom + MIN_THICKNESS {
                    continue;
                }
                mesh.push_quad(
                    side_corners(x0, z0, dx, dz, bottom, top),
                    [dx as f32, 0.0, dz as f32],
                    cell,
                );
            }
        }
    }
}

/// Corners of the vertical wall of column (x0, z0) facing direction (dx, dz), from `bottom` to
/// `top`, counter-clockwise seen from outside.
fn side_corners(x0: f32, z0: f32, dx: isize, dz: isize, bottom: f32, top: f32) -> [[f32; 3]; 4] {
    // Bottom edge of the wall, from `edge_a` to `edge_b`, chosen so that edge_a → up → edge_b
    // turns counter-clockwise seen from outside, i.e. edge_b − edge_a = normal × up.
    // Checked by the test `walls_face_outwards`.
    let (edge_a, edge_b) = match (dx, dz) {
        (1, 0) => ([x0 + 1.0, z0], [x0 + 1.0, z0 + 1.0]),
        (-1, 0) => ([x0, z0 + 1.0], [x0, z0]),
        (0, 1) => ([x0 + 1.0, z0 + 1.0], [x0, z0 + 1.0]),
        _ => ([x0, z0], [x0 + 1.0, z0]),
    };
    [
        [edge_a[0], bottom, edge_a[1]],
        [edge_a[0], top, edge_a[1]],
        [edge_b[0], top, edge_b[1]],
        [edge_b[0], bottom, edge_b[1]],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|i| a[i] - b[i])
    }

    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }

    fn assert_outwards(mesh: &MeshData) {
        for tri in mesh.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| mesh.vertices[tri[k] as usize]);
            let geometric = cross(sub(b.position, a.position), sub(c.position, a.position));
            let dot: f32 = (0..3).map(|i| geometric[i] * a.normal[i]).sum();
            assert!(
                dot > 0.0,
                "triangle {tri:?} wound against its normal {:?}",
                a.normal
            );
        }
    }

    #[test]
    fn dry_world_has_no_water() {
        let floor = Field2::filled(4, 4, 2.0);
        let mut mesh = MeshData::default();
        mesh_water(&floor, &floor.clone(), &mut mesh);
        assert_eq!(mesh.face_count(), 0);
    }

    #[test]
    fn pond_has_a_top_and_walls_down_to_the_dry_banks() {
        // One wet column in the middle of dry ground: 1 top + 4 walls.
        let floor = Field2::filled(3, 3, 2.0);
        let mut surface = floor.clone();
        surface.set(1, 1, 2.4);
        let mut mesh = MeshData::default();
        mesh_water(&floor, &surface, &mut mesh);
        assert_eq!(mesh.face_count(), 5);
        let top = mesh
            .vertices
            .iter()
            .map(|v| v.position[1])
            .fold(0.0, f32::max);
        assert_eq!(top, 2.4);
        assert_outwards(&mesh);
    }

    #[test]
    fn walls_face_outwards() {
        // A deep column in the middle of shallower water: walls on all 4 sides.
        let floor = Field2::filled(3, 3, 0.0);
        let mut surface = Field2::filled(3, 3, 0.5);
        surface.set(1, 1, 2.0);
        let mut mesh = MeshData::default();
        mesh_water(&floor, &surface, &mut mesh);
        assert_eq!(mesh.face_count(), 9 + 4);
        assert_outwards(&mesh);
    }

    #[test]
    fn no_wall_at_the_edge_of_the_world() {
        let floor = Field2::filled(1, 1, 0.0);
        let surface = Field2::filled(1, 1, 1.0);
        let mut mesh = MeshData::default();
        mesh_water(&floor, &surface, &mut mesh);
        assert_eq!(mesh.face_count(), 1);
    }

    #[test]
    fn flat_lake_has_no_inner_walls() {
        // 3 wet columns at the same level, dry banks around: 3 tops + 8 outer walls.
        let floor = Field2::filled(5, 3, 1.0);
        let mut surface = floor.clone();
        for x in 1..4 {
            surface.set(x, 1, 2.0);
        }
        let mut mesh = MeshData::default();
        mesh_water(&floor, &surface, &mut mesh);
        assert_eq!(mesh.face_count(), 3 + 8);
    }

    #[test]
    fn a_step_in_the_river_shows_a_small_wall() {
        // Two wet columns, the second 0.5 lower: the first has a wall from 1.5 up to 2.0.
        let floor = Field2::filled(2, 1, 1.0);
        let mut surface = Field2::filled(2, 1, 0.0);
        surface.set(0, 0, 2.0);
        surface.set(1, 0, 1.5);
        let mut mesh = MeshData::default();
        mesh_water(&floor, &surface, &mut mesh);
        let step_wall: Vec<_> = mesh
            .vertices
            .iter()
            .filter(|v| v.normal == [1.0, 0.0, 0.0] && v.position[0] == 1.0)
            .map(|v| v.position[1])
            .collect();
        assert!(
            step_wall.contains(&1.5) && step_wall.contains(&2.0),
            "{step_wall:?}"
        );
    }
}
