//! Solid things that are not in the world grid: stones of the ground cover. Each is a box,
//! filed by the columns it covers, so the player's collision test only looks at its own column
//! and its neighbours.

use std::collections::HashMap;

use glam::Vec3;
use world::vegetation::orient;
use world::{Plant, World};

/// Stones lower than this are stepped over without noticing (pebbles).
const MIN_HEIGHT: f32 = 0.2;

/// An axis-aligned box, in world cells.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn overlaps(&self, other: &Aabb) -> bool {
        self.min.cmplt(other.max).all() && other.min.cmplt(self.max).all()
    }
}

#[derive(Default)]
pub struct Obstacles {
    /// Boxes by column (x, z) they cover.
    by_column: HashMap<(i64, i64), Vec<Aabb>>,
}

impl Obstacles {
    /// The stones of the world.
    pub fn from_world(world: &World) -> Self {
        let mut obstacles = Self::default();
        for p in world.plants().iter().filter(|p| p.plant == Plant::Stone) {
            let model = world.model(p.plant, p.variant);
            let k = p.scale / model.resolution as f32;
            let [ax, ay, az] = model.anchor.map(|a| a as f32);
            let d = model.dims;
            // The model's box, oriented like the plant (quarter turns and mirror keep it
            // axis-aligned), then sized and placed.
            let corners = [(0.0 - ax, 0.0 - az), (d.nx as f32 - ax, d.nz as f32 - az)]
                .map(|(x, z)| orient(x, z, p.rotation, p.mirrored));
            let centre = Vec3::new(
                p.base[0] as f32 + 0.5 + p.offset[0],
                p.base[1] as f32,
                p.base[2] as f32 + 0.5 + p.offset[1],
            );
            let height = (d.ny as f32 - ay) * k;
            if height < MIN_HEIGHT {
                continue;
            }
            let (x0, x1) = (
                corners[0].0.min(corners[1].0),
                corners[0].0.max(corners[1].0),
            );
            let (z0, z1) = (
                corners[0].1.min(corners[1].1),
                corners[0].1.max(corners[1].1),
            );
            // Slightly smaller than the voxels' extent: stones are rounded.
            let shrink = 0.8;
            obstacles.add(Aabb {
                min: centre + Vec3::new(x0 * k * shrink, 0.0, z0 * k * shrink),
                max: centre + Vec3::new(x1 * k * shrink, height, z1 * k * shrink),
            });
        }
        obstacles
    }

    pub fn add(&mut self, aabb: Aabb) {
        for z in aabb.min.z.floor() as i64..=aabb.max.z.floor() as i64 {
            for x in aabb.min.x.floor() as i64..=aabb.max.x.floor() as i64 {
                self.by_column.entry((x, z)).or_default().push(aabb);
            }
        }
    }

    /// Whether `aabb` overlaps any obstacle.
    pub fn hits(&self, aabb: &Aabb) -> bool {
        for z in aabb.min.z.floor() as i64..=aabb.max.z.floor() as i64 {
            for x in aabb.min.x.floor() as i64..=aabb.max.x.floor() as i64 {
                if let Some(boxes) = self.by_column.get(&(x, z))
                    && boxes.iter().any(|b| b.overlaps(aabb))
                {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boxes_are_found_from_any_column_they_cover() {
        let mut o = Obstacles::default();
        let stone = Aabb {
            min: Vec3::new(4.6, 2.0, 7.2),
            max: Vec3::new(5.4, 2.6, 8.3),
        };
        o.add(stone);
        let probe = |x: f32, z: f32| Aabb {
            min: Vec3::new(x - 0.1, 2.1, z - 0.1),
            max: Vec3::new(x + 0.1, 2.3, z + 0.1),
        };
        assert!(o.hits(&probe(5.0, 8.0)));
        assert!(o.hits(&probe(4.7, 7.3)));
        assert!(!o.hits(&probe(6.0, 8.0)));
        // Above the stone: free.
        let above = Aabb {
            min: Vec3::new(4.9, 2.7, 7.9),
            max: Vec3::new(5.1, 4.0, 8.1),
        };
        assert!(!o.hits(&above));
    }
}
