//! The interface drawn over the image: flat rectangles and text, in screen pixels. Text uses a
//! small pixel font defined here, in code (no font file): each glyph is a 5 × 9 grid of bits,
//! and French accents are drawn above or under their base letter.
//!
//! The game describes the interface each frame with a `Ui` (a list of coloured triangles);
//! `UiPass` draws it last, over the final image, with alpha blending.

use wgpu::util::DeviceExt;

/// A corner of a triangle of the interface.
///
/// WGSL side (`shaders/ui.wgsl`, `UiVertex`):
/// ```wgsl
/// @location(0) position: vec2<f32>,  // offset 0: clip space
/// @location(1) color: vec4<f32>,     // offset 8: linear rgb, alpha
/// ```
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

impl UiVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<UiVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Glyph width in font pixels, and advance from one character to the next.
pub const GLYPH_WIDTH: f32 = 5.0;
pub const ADVANCE: f32 = 6.0;
/// Height of a line of text in font pixels (capitals are 7 tall; descenders and accents
/// overflow a little).
pub const LINE: f32 = 11.0;

/// An interface being described for one frame, in pixels from the top left corner.
pub struct Ui {
    width: f32,
    height: f32,
    vertices: Vec<UiVertex>,
}

impl Ui {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: width as f32,
            height: height as f32,
            vertices: Vec::new(),
        }
    }

    pub fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    pub fn vertices(&self) -> &[UiVertex] {
        &self.vertices
    }

    /// A filled rectangle. `color`: linear rgb and alpha.
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        let clip = |px: f32, py: f32| [px / self.width * 2.0 - 1.0, 1.0 - py / self.height * 2.0];
        let (a, b, c, d) = (
            clip(x, y),
            clip(x + w, y),
            clip(x + w, y + h),
            clip(x, y + h),
        );
        for position in [a, b, c, a, c, d] {
            self.vertices.push(UiVertex { position, color });
        }
    }

    /// A rectangle outline `thickness` pixels wide.
    pub fn frame(&mut self, x: f32, y: f32, w: f32, h: f32, thickness: f32, color: [f32; 4]) {
        self.rect(x, y, w, thickness, color);
        self.rect(x, y + h - thickness, w, thickness, color);
        self.rect(x, y, thickness, h, color);
        self.rect(x + w - thickness, y, thickness, h, color);
    }

    /// Width in pixels of `text` at `scale` screen pixels per font pixel.
    pub fn text_width(text: &str, scale: f32) -> f32 {
        let n = text.chars().count() as f32;
        (n * ADVANCE - (ADVANCE - GLYPH_WIDTH)).max(0.0) * scale
    }

    /// Writes `text` with its top left corner at (x, y). Unknown characters show as a box.
    pub fn text(&mut self, x: f32, y: f32, text: &str, scale: f32, color: [f32; 4]) {
        let mut cursor = x;
        for ch in text.chars() {
            let (rows, accent) = glyph(ch);
            for (r, bits) in rows.iter().enumerate() {
                self.row(cursor, y + r as f32 * scale, *bits, scale, color);
            }
            if let Some((marks, row)) = accent {
                for (r, bits) in marks.iter().enumerate() {
                    self.row(
                        cursor,
                        y + (row + r as i32) as f32 * scale,
                        *bits,
                        scale,
                        color,
                    );
                }
            }
            cursor += ADVANCE * scale;
        }
    }

    /// One row of a glyph: runs of lit pixels become one rectangle each.
    fn row(&mut self, x: f32, y: f32, bits: u8, scale: f32, color: [f32; 4]) {
        let mut column = 0;
        while column < 5 {
            if bits & (0x10 >> column) == 0 {
                column += 1;
                continue;
            }
            let start = column;
            while column < 5 && bits & (0x10 >> column) != 0 {
                column += 1;
            }
            self.rect(
                x + start as f32 * scale,
                y,
                (column - start) as f32 * scale,
                scale,
                color,
            );
        }
    }

    /// Text with a dark shadow one pixel down and right: readable on any background.
    pub fn text_shadowed(&mut self, x: f32, y: f32, text: &str, scale: f32, color: [f32; 4]) {
        let shadow = [0.0, 0.0, 0.0, color[3] * 0.6];
        self.text(x + scale * 0.5, y + scale * 0.5, text, scale, shadow);
        self.text(x, y, text, scale, color);
    }
}

// ---------- Font ----------

/// Accents, as two rows drawn above the letter (or one under, for the cedilla).
const ACUTE: [u8; 2] = [0x02, 0x04];
const GRAVE: [u8; 2] = [0x08, 0x04];
const CIRCUMFLEX: [u8; 2] = [0x04, 0x0A];
const DIAERESIS: [u8; 2] = [0x0A, 0x00];
const CEDILLA: [u8; 2] = [0x04, 0x08];

/// Rows of a glyph (bit 4 = leftmost column; rows 0–6 the body, 7–8 descenders), and an
/// accent with the row it starts at (negative: above a capital).
fn glyph(ch: char) -> ([u8; 9], Option<([u8; 2], i32)>) {
    let lower = |base: char, accent: [u8; 2]| (base_glyph(base), Some((accent, 0)));
    let upper = |base: char, accent: [u8; 2]| (base_glyph(base), Some((accent, -3)));
    match ch {
        'é' => lower('e', ACUTE),
        'è' => lower('e', GRAVE),
        'ê' => lower('e', CIRCUMFLEX),
        'ë' => lower('e', DIAERESIS),
        'à' => lower('a', GRAVE),
        'â' => lower('a', CIRCUMFLEX),
        'ô' => lower('o', CIRCUMFLEX),
        'ö' => lower('o', DIAERESIS),
        'ù' => lower('u', GRAVE),
        'û' => lower('u', CIRCUMFLEX),
        'ü' => lower('u', DIAERESIS),
        // Dotless i under the accent.
        'î' => (
            [0, 0, 0x0C, 0x04, 0x04, 0x04, 0x0E, 0, 0],
            Some((CIRCUMFLEX, 0)),
        ),
        'ï' => (
            [0, 0, 0x0C, 0x04, 0x04, 0x04, 0x0E, 0, 0],
            Some((DIAERESIS, 0)),
        ),
        'ç' => (base_glyph('c'), Some((CEDILLA, 7))),
        'É' => upper('E', ACUTE),
        'È' => upper('E', GRAVE),
        'Ê' => upper('E', CIRCUMFLEX),
        'À' => upper('A', GRAVE),
        'Ç' => (base_glyph('C'), Some((CEDILLA, 7))),
        _ => (base_glyph(ch), None),
    }
}

fn base_glyph(ch: char) -> [u8; 9] {
    let r = |rows: &[u8]| {
        let mut out = [0u8; 9];
        out[..rows.len()].copy_from_slice(rows);
        out
    };
    match ch {
        'A' => r(&[0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11]),
        'B' => r(&[0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E]),
        'C' => r(&[0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E]),
        'D' => r(&[0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E]),
        'E' => r(&[0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F]),
        'F' => r(&[0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10]),
        'G' => r(&[0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F]),
        'H' => r(&[0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11]),
        'I' => r(&[0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E]),
        'J' => r(&[0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C]),
        'K' => r(&[0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11]),
        'L' => r(&[0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F]),
        'M' => r(&[0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11]),
        'N' => r(&[0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11]),
        'O' => r(&[0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E]),
        'P' => r(&[0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10]),
        'Q' => r(&[0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D]),
        'R' => r(&[0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11]),
        'S' => r(&[0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E]),
        'T' => r(&[0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04]),
        'U' => r(&[0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E]),
        'V' => r(&[0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04]),
        'W' => r(&[0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A]),
        'X' => r(&[0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11]),
        'Y' => r(&[0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04]),
        'Z' => r(&[0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F]),
        'a' => r(&[0, 0, 0x0E, 0x01, 0x0F, 0x11, 0x0F]),
        'b' => r(&[0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x1E]),
        'c' => r(&[0, 0, 0x0E, 0x10, 0x10, 0x11, 0x0E]),
        'd' => r(&[0x01, 0x01, 0x0D, 0x13, 0x11, 0x11, 0x0F]),
        'e' => r(&[0, 0, 0x0E, 0x11, 0x1F, 0x10, 0x0E]),
        'f' => r(&[0x06, 0x09, 0x08, 0x1C, 0x08, 0x08, 0x08]),
        'g' => r(&[0, 0, 0x0F, 0x11, 0x11, 0x11, 0x0F, 0x01, 0x0E]),
        'h' => r(&[0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x11]),
        'i' => r(&[0x04, 0, 0x0C, 0x04, 0x04, 0x04, 0x0E]),
        'j' => r(&[0x02, 0, 0x06, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C]),
        'k' => r(&[0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12]),
        'l' => r(&[0x0C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E]),
        'm' => r(&[0, 0, 0x1A, 0x15, 0x15, 0x11, 0x11]),
        'n' => r(&[0, 0, 0x16, 0x19, 0x11, 0x11, 0x11]),
        'o' => r(&[0, 0, 0x0E, 0x11, 0x11, 0x11, 0x0E]),
        'p' => r(&[0, 0, 0x1E, 0x11, 0x11, 0x11, 0x1E, 0x10, 0x10]),
        'q' => r(&[0, 0, 0x0F, 0x11, 0x11, 0x11, 0x0F, 0x01, 0x01]),
        'r' => r(&[0, 0, 0x16, 0x19, 0x10, 0x10, 0x10]),
        's' => r(&[0, 0, 0x0F, 0x10, 0x0E, 0x01, 0x1E]),
        't' => r(&[0x08, 0x08, 0x1C, 0x08, 0x08, 0x09, 0x06]),
        'u' => r(&[0, 0, 0x11, 0x11, 0x11, 0x13, 0x0D]),
        'v' => r(&[0, 0, 0x11, 0x11, 0x11, 0x0A, 0x04]),
        'w' => r(&[0, 0, 0x11, 0x11, 0x15, 0x15, 0x0A]),
        'x' => r(&[0, 0, 0x11, 0x0A, 0x04, 0x0A, 0x11]),
        'y' => r(&[0, 0, 0x11, 0x11, 0x11, 0x11, 0x0F, 0x01, 0x0E]),
        'z' => r(&[0, 0, 0x1F, 0x02, 0x04, 0x08, 0x1F]),
        '0' => r(&[0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E]),
        '1' => r(&[0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E]),
        '2' => r(&[0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F]),
        '3' => r(&[0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E]),
        '4' => r(&[0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02]),
        '5' => r(&[0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E]),
        '6' => r(&[0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E]),
        '7' => r(&[0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08]),
        '8' => r(&[0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E]),
        '9' => r(&[0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C]),
        ' ' => [0; 9],
        '.' => r(&[0, 0, 0, 0, 0, 0x0C, 0x0C]),
        ',' => r(&[0, 0, 0, 0, 0, 0x0C, 0x04, 0x08]),
        ':' => r(&[0, 0x0C, 0x0C, 0, 0x0C, 0x0C, 0]),
        ';' => r(&[0, 0x0C, 0x0C, 0, 0x0C, 0x04, 0x08]),
        '\'' | '’' => r(&[0x0C, 0x04, 0x08]),
        '!' => r(&[0x04, 0x04, 0x04, 0x04, 0x04, 0, 0x04]),
        '?' => r(&[0x0E, 0x11, 0x01, 0x02, 0x04, 0, 0x04]),
        '-' => r(&[0, 0, 0, 0x1F]),
        '+' => r(&[0, 0x04, 0x04, 0x1F, 0x04, 0x04]),
        '/' => r(&[0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10]),
        '(' => r(&[0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02]),
        ')' => r(&[0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08]),
        '%' => r(&[0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03]),
        '°' => r(&[0x0C, 0x12, 0x12, 0x0C]),
        '×' => r(&[0, 0x11, 0x0A, 0x04, 0x0A, 0x11]),
        // Unknown: an empty box.
        _ => r(&[0x1F, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1F]),
    }
}

// ---------- Drawing ----------

/// Draws a `Ui` over the final image.
pub struct UiPass {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    capacity: usize,
    count: u32,
}

impl UiPass {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("../shaders/ui.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_ui"),
                compilation_options: Default::default(),
                buffers: &[Some(UiVertex::layout())],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_ui"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let capacity = 4096;
        Self {
            pipeline,
            buffer: create_buffer(device, capacity),
            capacity,
            count: 0,
        }
    }

    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, vertices: &[UiVertex]) {
        if vertices.len() > self.capacity {
            self.capacity = vertices.len().next_power_of_two();
            self.buffer = create_buffer(device, self.capacity);
        }
        if !vertices.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(vertices));
        }
        self.count = vertices.len() as u32;
    }

    /// Draws over `target`, keeping what is there.
    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        if self.count == 0 {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.buffer.slice(..));
        pass.draw(0..self.count, 0..1);
    }
}

fn create_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("ui vertices"),
        contents: &vec![0u8; capacity * std::mem::size_of::<UiVertex>()],
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertex_layout_matches_wgsl() {
        assert_eq!(std::mem::size_of::<UiVertex>(), 24);
    }

    #[test]
    fn every_letter_of_the_game_texts_has_a_glyph() {
        let unknown = base_glyph('\u{1}');
        let texts = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 .,:;'!?-+/()%°";
        for ch in texts.chars() {
            assert!(ch == ' ' || base_glyph(ch) != unknown, "{ch}");
        }
        for ch in "éèêëàâôöùûüîïçÉÈÊÀÇ".chars() {
            assert!(glyph(ch).1.is_some(), "{ch} has no accent");
        }
    }

    #[test]
    fn text_is_made_of_rectangles_inside_its_box() {
        let mut ui = Ui::new(200, 100);
        ui.text(10.0, 10.0, "Été", 2.0, [1.0; 4]);
        assert!(!ui.vertices().is_empty());
        let width = Ui::text_width("Été", 2.0);
        for v in ui.vertices() {
            let x = (v.position[0] + 1.0) / 2.0 * 200.0;
            assert!((10.0 - 1e-3..=10.0 + width + 1e-3).contains(&x));
        }
    }
}
