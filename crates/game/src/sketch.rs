//! Sketches for the notebook, drawn from what was really on screen: the scene image, before
//! the interface, turned into ink on paper. Contours where the light changes sharply (edge
//! detection), hatching where it is dark, one direction for the half-tones and crossed for the
//! deep shadows, as a pen would.

/// Size of a sketch, in ink dots.
pub const WIDTH: usize = 96;
pub const HEIGHT: usize = 60;

/// Ink per dot, 0 (bare paper) to 255 (full ink), rows from top to bottom.
#[derive(Clone)]
pub struct Sketch {
    pub ink: Vec<u8>,
}

/// Draws a sketch from an image (linear RGB, rows from top to bottom): its middle, where the
/// eye was, cropped to the sketch's proportions.
pub fn draw(width: u32, height: u32, pixels: &[[f32; 3]]) -> Sketch {
    let (w, h) = (width as usize, height as usize);
    // Work at twice the final size, then keep the strongest stroke of each 2 × 2.
    let (sw, sh) = (WIDTH * 2, HEIGHT * 2);
    let aspect = WIDTH as f32 / HEIGHT as f32;
    let (full_w, full_h) = if w as f32 / h as f32 > aspect {
        (h as f32 * aspect, h as f32)
    } else {
        (w as f32, w as f32 / aspect)
    };
    // The middle 70 %: what one looks at, not the corners.
    let (crop_w, crop_h) = ((full_w * 0.7) as usize, (full_h * 0.7) as usize);
    let (x0, y0) = ((w - crop_w) / 2, (h - crop_h) / 2);
    // Brightness as seen: tone-mapped (light compressed like the eye does) and gamma.
    let mut light = vec![0.0f32; sw * sh];
    for sy in 0..sh {
        for sx in 0..sw {
            let (ax, bx) = (x0 + sx * crop_w / sw, x0 + (sx + 1) * crop_w / sw);
            let (ay, by) = (y0 + sy * crop_h / sh, y0 + (sy + 1) * crop_h / sh);
            let (mut sum, mut count) = (0.0, 0.0);
            for y in ay..by.max(ay + 1).min(h) {
                for x in ax..bx.max(ax + 1).min(w) {
                    let [r, g, b] = pixels[y * w + x];
                    let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    sum += (l / (1.0 + l)).max(0.0).powf(1.0 / 2.2);
                    count += 1.0;
                }
            }
            light[sy * sw + sx] = if count > 0.0 { sum / count } else { 1.0 };
        }
    }
    // Blurred twice: the pen follows forms, not every blade of grass.
    for _ in 0..2 {
        light = blur(&light, sw, sh);
    }
    let at = |x: i64, y: i64| {
        let (x, y) = (x.clamp(0, sw as i64 - 1), y.clamp(0, sh as i64 - 1));
        light[y as usize * sw + x as usize]
    };
    // Contours: Sobel gradient of the brightness.
    let mut edges = vec![0.0f32; sw * sh];
    for y in 0..sh as i64 {
        for x in 0..sw as i64 {
            let gx = at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2.0 * at(x - 1, y)
                - at(x - 1, y + 1);
            let gy = at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2.0 * at(x, y - 1)
                - at(x + 1, y - 1);
            edges[y as usize * sw + x as usize] = (gx * gx + gy * gy).sqrt();
        }
    }
    // Only the strongest contours of this image are drawn (about one dot in eight), and the
    // shadows hatched relative to its own range: a night sketch is not a black page.
    let percentile = |values: &[f32], q: f32| {
        let mut sorted = values.to_vec();
        sorted.sort_by(f32::total_cmp);
        sorted[((sorted.len() - 1) as f32 * q) as usize]
    };
    let strong = percentile(&edges, 0.88).max(0.03);
    let (low, high) = (percentile(&light, 0.05), percentile(&light, 0.95));
    let span = (high - low).max(0.05);
    let mut ink = vec![0u8; WIDTH * HEIGHT];
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let mut edge: f32 = 0.0;
            let mut bright = 0.0;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let i = (2 * y + dy) * sw + 2 * x + dx;
                edge = edge.max(smoothstep(strong, strong * 1.8, edges[i]));
                bright += light[i] / 4.0;
            }
            let dark = 1.0 - ((bright - low) / span).clamp(0.0, 1.0);
            let hatch = if dark > 0.9 && (x + 2 * HEIGHT - y).is_multiple_of(3) {
                0.7
            } else if dark > 0.78 && (x + y).is_multiple_of(3) {
                0.55
            } else {
                0.0
            };
            // The drawing fades out towards its edges, as a sketch does.
            let (u, v) = (
                (x as f32 + 0.5) / WIDTH as f32 * 2.0 - 1.0,
                (y as f32 + 0.5) / HEIGHT as f32 * 2.0 - 1.0,
            );
            let reach = 1.0 - smoothstep(0.7, 1.05, (u * u + v * v).sqrt());
            let value = edge.max(hatch) * reach;
            ink[y * WIDTH + x] = (value.min(1.0) * 255.0) as u8;
        }
    }
    Sketch { ink }
}

/// 3 × 3 box blur.
fn blur(values: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0; values.len()];
    for y in 0..h {
        for x in 0..w {
            let mut sum = 0.0;
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (cx, cy) = (
                        (x as i64 + dx).clamp(0, w as i64 - 1) as usize,
                        (y as i64 + dy).clamp(0, h as i64 - 1) as usize,
                    );
                    sum += values[cy * w + cx];
                }
            }
            out[y * w + x] = sum / 9.0;
        }
    }
    out
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dark_disc_on_light_ground_gives_a_ring_of_ink_and_hatching_inside() {
        let (w, h) = (320u32, 200u32);
        let pixels: Vec<[f32; 3]> = (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f32, (i / w) as f32);
                let inside = (x - 160.0).hypot(y - 100.0) < 40.0;
                if inside { [0.02; 3] } else { [0.8; 3] }
            })
            .collect();
        let sketch = draw(w, h, &pixels);
        let ink = |x: usize, y: usize| sketch.ink[y * WIDTH + x];
        // Paper far from the disc, ink on its rim, some ink (hatching) inside.
        assert_eq!(ink(5, 5), 0);
        let rim = (0..WIDTH)
            .map(|x| ink(x, HEIGHT / 2))
            .filter(|&v| v > 200)
            .count();
        assert!(rim >= 2, "{rim}");
        let inside = (40..56)
            .map(|x| ink(x, HEIGHT / 2))
            .filter(|&v| v > 0)
            .count();
        assert!(inside > 2, "{inside}");
    }
}
