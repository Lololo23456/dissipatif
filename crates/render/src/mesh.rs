//! Vertex format of the voxel meshes and their GPU buffers.

/// The six axis directions, i.e. the outward normals of the faces of a cube.
pub const FACE_NORMALS: [[i32; 3]; 6] = [
    [1, 0, 0],
    [-1, 0, 0],
    [0, 1, 0],
    [0, -1, 0],
    [0, 0, 1],
    [0, 0, -1],
];

/// Corners of a face in winding order, as steps (su, sv) along its tangents (u, v).
pub const FACE_CORNERS: [(i32, i32); 4] = [(0, 0), (1, 0), (1, 1), (0, 1)];

/// Tangents (u, v) of the face with outward normal `normal`, chosen so that u × v = normal:
/// walking `FACE_CORNERS` is then counter-clockwise seen from the normal side.
pub fn face_tangents(normal: [i32; 3]) -> ([i32; 3], [i32; 3]) {
    match normal {
        [1, 0, 0] => ([0, 1, 0], [0, 0, 1]),
        [-1, 0, 0] => ([0, 0, 1], [0, 1, 0]),
        [0, 1, 0] => ([0, 0, 1], [1, 0, 0]),
        [0, -1, 0] => ([1, 0, 0], [0, 0, 1]),
        [0, 0, 1] => ([1, 0, 0], [0, 1, 0]),
        [0, 0, -1] => ([0, 1, 0], [1, 0, 0]),
        _ => panic!("not an axis direction: {normal:?}"),
    }
}

/// Largest grid size per axis: each cell coordinate is packed on 10 bits.
pub const MAX_GRID_SIZE: u32 = 1 << 10;

/// One vertex of a voxel face.
///
/// Vertex buffers are not subject to the 16-byte alignment of uniforms: the layout below
/// (`vertex_layout`) tells the GPU exactly where each attribute sits.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// World position (one voxel = one unit).
    pub position: [f32; 3],
    /// Outward face normal.
    pub normal: [f32; 3],
    /// Grid cell the face belongs to, packed as `x | y << 10 | z << 20` (see `pack_cell`).
    /// The fragment shader uses it to read the cell's value in the field texture.
    pub cell: u32,
    /// Ambient occlusion at this corner: 1 = fully open, 0 = buried in a corner.
    pub ao: f32,
}

/// Packs cell coordinates into one `u32`. Unpacked in `shaders/voxel.wgsl`.
pub fn pack_cell([x, y, z]: [u32; 3]) -> u32 {
    debug_assert!(x < MAX_GRID_SIZE && y < MAX_GRID_SIZE && z < MAX_GRID_SIZE);
    x | (y << 10) | (z << 20)
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 4] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Uint32, 3 => Float32];

    /// How the GPU reads a buffer of `Vertex`: stride, then each attribute's
    /// `@location`, format and offset.
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// CPU-side mesh: four vertices per face, two triangles per face.
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Empties the mesh but keeps its memory, so rebuilding it does not allocate.
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }

    /// Number of square faces (two triangles each).
    pub fn face_count(&self) -> usize {
        self.indices.len() / 6
    }

    /// Appends the face of grid cell `cell`, i.e. of the unit cube whose minimum corner is `cell`,
    /// with outward normal `normal`, one of the six axis directions.
    ///
    /// `ao` gives the ambient occlusion of each corner, in `FACE_CORNERS` order.
    ///
    /// Triangles are counter-clockwise seen from outside, so back-face culling keeps them.
    pub fn push_face(&mut self, cell: [u32; 3], normal: [i32; 3], ao: [f32; 4]) {
        let (u, v) = face_tangents(normal);
        let base = self.vertices.len() as u32;
        let n = normal.map(|c| c as f32);
        let packed = pack_cell(cell);
        for (k, (su, sv)) in FACE_CORNERS.into_iter().enumerate() {
            // Corner offset in the cube [0,1]³: the face sits on the side the normal points to.
            let corner: [i32; 3] =
                std::array::from_fn(|i| normal[i].max(0) + su * u[i] + sv * v[i]);
            self.vertices.push(Vertex {
                position: std::array::from_fn(|i| cell[i] as f32 + corner[i] as f32),
                normal: n,
                cell: packed,
                ao: ao[k],
            });
        }
        // A quad is two triangles split along a diagonal, and the GPU interpolates AO inside
        // each triangle. Splitting along the diagonal whose ends are the most alike keeps the
        // shading symmetric; the wrong one shows a dark streak across the face.
        // Both splits keep the counter-clockwise order.
        let quad = if (ao[0] - ao[2]).abs() <= (ao[1] - ao[3]).abs() {
            [0, 1, 2, 0, 2, 3]
        } else {
            [1, 2, 3, 1, 3, 0]
        };
        self.indices.extend(quad.map(|i| base + i));
    }

    /// Appends a quad with arbitrary corners, given counter-clockwise seen from the side the
    /// normal points to. For surfaces that are not whole voxel faces, such as water at its
    /// exact height. `cell` is packed with `pack_cell` and selects the colour texel.
    pub fn push_quad(&mut self, corners: [[f32; 3]; 4], normal: [f32; 3], cell: [u32; 3]) {
        let base = self.vertices.len() as u32;
        let packed = pack_cell(cell);
        for position in corners {
            self.vertices.push(Vertex {
                position,
                normal,
                cell: packed,
                ao: 1.0,
            });
        }
        self.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
    }

    /// A single isolated cube (no occlusion).
    pub fn cube(cell: [u32; 3]) -> Self {
        let mut mesh = Self::default();
        for normal in FACE_NORMALS {
            mesh.push_face(cell, normal, [1.0; 4]);
        }
        mesh
    }
}

/// Mesh uploaded to the GPU. Its buffers are reused from one update to the next and only
/// reallocated when the mesh outgrows them, so remeshing every frame does not allocate.
pub struct GpuMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
}

impl GpuMesh {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, data: &MeshData) -> Self {
        let mut mesh = Self {
            vertex_buffer: create_buffer(device, "voxel vertices", wgpu::BufferUsages::VERTEX, 0),
            index_buffer: create_buffer(device, "voxel indices", wgpu::BufferUsages::INDEX, 0),
            index_count: 0,
        };
        mesh.update(device, queue, data);
        mesh
    }

    pub fn update(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &MeshData) {
        let vertices: &[u8] = bytemuck::cast_slice(&data.vertices);
        let indices: &[u8] = bytemuck::cast_slice(&data.indices);
        // Too small: replace with a buffer 50 % larger than needed, so a slowly growing
        // structure does not trigger a reallocation every frame.
        if vertices.len() as u64 > self.vertex_buffer.size() {
            let size = vertices.len() as u64 * 3 / 2;
            self.vertex_buffer =
                create_buffer(device, "voxel vertices", wgpu::BufferUsages::VERTEX, size);
        }
        if indices.len() as u64 > self.index_buffer.size() {
            let size = indices.len() as u64 * 3 / 2;
            self.index_buffer =
                create_buffer(device, "voxel indices", wgpu::BufferUsages::INDEX, size);
        }
        // `write_buffer` requires sizes that are multiples of 4: true for 32-byte vertices
        // and 4-byte indices.
        queue.write_buffer(&self.vertex_buffer, 0, vertices);
        queue.write_buffer(&self.index_buffer, 0, indices);
        self.index_count = data.indices.len() as u32;
    }
}

/// `usage` declares what the buffer will be used for; COPY_DST allows `queue.write_buffer`.
fn create_buffer(
    device: &wgpu::Device,
    label: &str,
    usage: wgpu::BufferUsages,
    size: u64,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        // Multiple of 4 (copy alignment), and never empty.
        size: size.next_multiple_of(4).max(4),
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
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

    #[test]
    fn vertex_is_tightly_packed() {
        assert_eq!(std::mem::size_of::<Vertex>(), 32);
    }

    #[test]
    fn cube_triangles_face_outwards() {
        let mesh = MeshData::cube([2, 3, 4]);
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
        let centre = [2.5, 3.5, 4.5];
        for tri in mesh.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2].map(|k| mesh.vertices[tri[k] as usize]);
            // Counter-clockwise winding ⇒ geometric normal (b−a)×(c−a) along the stored normal.
            let geometric = cross(sub(b.position, a.position), sub(c.position, a.position));
            let dot: f32 = (0..3).map(|i| geometric[i] * a.normal[i]).sum();
            assert!(dot > 0.0, "triangle {tri:?} is wound clockwise");
            // And the stored normal points away from the cube centre.
            let outward = sub(a.position, centre);
            let dot: f32 = (0..3).map(|i| outward[i] * a.normal[i]).sum();
            assert!(dot > 0.0, "normal of {tri:?} points inwards");
        }
    }

    #[test]
    fn both_diagonal_splits_face_outwards() {
        for ao in [[1.0, 0.0, 1.0, 0.0], [0.0, 1.0, 0.0, 1.0]] {
            let mut mesh = MeshData::default();
            for normal in FACE_NORMALS {
                mesh.push_face([0, 0, 0], normal, ao);
            }
            for tri in mesh.indices.chunks(3) {
                let [a, b, c] = [0, 1, 2].map(|k| mesh.vertices[tri[k] as usize]);
                let geometric = cross(sub(b.position, a.position), sub(c.position, a.position));
                let dot: f32 = (0..3).map(|i| geometric[i] * a.normal[i]).sum();
                assert!(dot > 0.0, "ao {ao:?}: triangle {tri:?} is wound clockwise");
            }
        }
    }

    #[test]
    fn single_odd_corner_stays_out_of_the_diagonal() {
        // Only corner 0 differs: the split must not run through it, whether it is the dark
        // one or the bright one, or its shade would leak across the whole face.
        for ao in [[0.0, 1.0, 1.0, 1.0], [1.0, 0.0, 0.0, 0.0]] {
            let mut mesh = MeshData::default();
            mesh.push_face([0, 0, 0], [0, 1, 0], ao);
            assert_eq!(&mesh.indices[..3], &[1, 2, 3], "ao {ao:?}");
        }
    }

    #[test]
    fn split_follows_the_most_alike_diagonal() {
        let mut mesh = MeshData::default();
        // Corners 1 and 3 are equally dark, 0 and 2 differ: split along 1–3.
        mesh.push_face([0, 0, 0], [0, 1, 0], [1.0, 0.0, 0.5, 0.0]);
        assert_eq!(&mesh.indices[..3], &[1, 2, 3]);
    }

    #[test]
    fn every_vertex_knows_its_cell() {
        let mesh = MeshData::cube([5, 0, 1023]);
        let expected = 5 | (1023 << 20);
        assert!(mesh.vertices.iter().all(|v| v.cell == expected));
    }

    #[test]
    fn cube_stays_inside_its_cell() {
        let mesh = MeshData::cube([2, 3, 4]);
        for v in &mesh.vertices {
            for (i, lo) in [2.0, 3.0, 4.0].into_iter().enumerate() {
                assert!(v.position[i] == lo || v.position[i] == lo + 1.0);
            }
        }
    }
}
