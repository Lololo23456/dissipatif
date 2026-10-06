// Post-processing: one full-screen pass that reworks the image of the scene before display.
// The scene was drawn in linear colours into an HDR image (values may exceed 1); this pass
// adds, in order: tilt-shift blur, bloom, colour grading, a soft shoulder for highlights,
// vignette and film grain. The output target is sRGB: the GPU converts on write.

// Rust side: `PostUniform` in src/post.rs.
struct Post {
    time: vec4<f32>,  // offset 0: x = seconds, y z = camera target along screen right / up
                      // on the ground (cells), w = camera distance (cells)
    shadow_tint: vec4<f32>,  // offset 16: tint of the darks, w = saturation
    light_tint: vec4<f32>,   // offset 32: tint of the lights, w = exposure
    night: vec4<f32>,        // offset 48: x = darkness of the night, y = weariness (0 to 1)
}

@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
@group(0) @binding(2) var<uniform> post: Post;

struct FullScreen {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// One triangle larger than the screen, (−1,−1), (3,−1), (−1,3): it covers every pixel with no
// vertex buffer, and without the seam of two triangles.
@vertex
fn vs_fullscreen(@builtin(vertex_index) index: u32) -> FullScreen {
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: FullScreen;
    out.position = vec4<f32>(corner * 2.0 - 1.0, 0.0, 1.0);
    // Clip space y goes up, texture v goes down.
    out.uv = vec2<f32>(corner.x, 1.0 - corner.y);
    return out;
}

// ---------- Tilt-shift ----------

// Sharp band around the screen's centre line (where the naturalist is), blurred above and
// below: the eye reads it as a close, miniature scene.
const FOCUS_CENTRE: f32 = 0.52;
const FOCUS_HALF_WIDTH: f32 = 0.16;
const FOCUS_FADE: f32 = 0.34;
// Largest blur radius, in pixels at 1080 lines (scaled with the image height).
const MAX_BLUR: f32 = 5.5;
const DISC_TAPS: i32 = 16;
// Golden angle: successive taps turn by it, spreading evenly over a disc (Vogel's spiral).
const GOLDEN_ANGLE: f32 = 2.39996;

fn vogel(k: i32, taps: i32) -> vec2<f32> {
    let r = sqrt((f32(k) + 0.5) / f32(taps));
    let a = f32(k) * GOLDEN_ANGLE;
    return vec2<f32>(cos(a), sin(a)) * r;
}

fn disc_blur(uv: vec2<f32>, radius: vec2<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    for (var k = 0; k < DISC_TAPS; k++) {
        sum += textureSampleLevel(scene, scene_sampler, uv + vogel(k, DISC_TAPS) * radius, 0.0).rgb;
    }
    return sum / f32(DISC_TAPS);
}

// ---------- Bloom ----------

// Light above this luminance bleeds around (water glints, pale flowers in the sun).
const BLOOM_THRESHOLD: f32 = 0.85;
const BLOOM_STRENGTH: f32 = 0.35;
// Radius, as a share of the image height.
const BLOOM_RADIUS: f32 = 0.025;
const BLOOM_TAPS: i32 = 24;

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn bloom(uv: vec2<f32>, texel: vec2<f32>, height: f32) -> vec3<f32> {
    let radius = texel * BLOOM_RADIUS * height;
    var sum = vec3<f32>(0.0);
    for (var k = 0; k < BLOOM_TAPS; k++) {
        let c = textureSampleLevel(scene, scene_sampler, uv + vogel(k, BLOOM_TAPS) * radius, 0.0).rgb;
        sum += c * max(luminance(c) - BLOOM_THRESHOLD, 0.0);
    }
    return sum / f32(BLOOM_TAPS) * BLOOM_STRENGTH;
}

// ---------- Grading ----------

// Split toning, as in Firewatch: shadows lean to cool violet-blue, lights to warm amber. The
// tints, saturation and exposure follow the hour of the day (src/sky.rs).
// Above this value, highlights roll off softly towards 1 instead of clipping.
const SHOULDER: f32 = 0.8;

fn grade(c: vec3<f32>) -> vec3<f32> {
    let exposed = c * post.light_tint.w;
    let l = luminance(exposed);
    var graded = exposed * mix(post.shadow_tint.rgb, post.light_tint.rgb, smoothstep(0.03, 0.45, l));
    graded = max(mix(vec3<f32>(luminance(graded)), graded, post.shadow_tint.w), vec3<f32>(0.0));
    // Soft shoulder: identity below SHOULDER, exponential approach to 1 above.
    let room = 1.0 - SHOULDER;
    let rolled = SHOULDER + room * (1.0 - exp(-(graded - SHOULDER) / room));
    return select(graded, rolled, graded > vec3<f32>(SHOULDER));
}

// ---------- Foreground dust ----------

// Specks of dust floating between the camera and the scene, out of focus: soft discs of warm
// light drifting slowly. Each layer is a grid over the screen with at most one speck per
// cell, at a random place; a pixel looks at its cell and the eight around. The layers move
// faster than the ground when the camera moves (parallax): they are closer.
const DUST_LAYERS: i32 = 2;
const DUST_COLOR: vec3<f32> = vec3<f32>(1.0, 0.9, 0.7);
const DUST_STRENGTH: f32 = 0.3;
// Share of grid cells holding a speck.
const DUST_DENSITY: f32 = 0.22;

fn hash3(c: vec2<i32>, layer: i32, salt: u32) -> f32 {
    var h = bitcast<u32>(c.x) * 0x8da6b343u ^ bitcast<u32>(c.y) * 0xd8163841u
        ^ bitcast<u32>(layer) * 0xcb1ab31fu ^ salt * 0x165667b1u;
    h = (h ^ (h >> 13u)) * 0x5bd1e995u;
    h = h ^ (h >> 15u);
    return f32(h) / 4294967295.0;
}

// `p`: screen position, aspect corrected (height 1).
fn dust(p: vec2<f32>, layer: i32, t: f32) -> f32 {
    let near = f32(layer);
    // Cells per screen height: the closer layer has fewer, larger specks.
    let scale = mix(5.0, 2.5, near);
    // The ground moves on screen by 1 / (view height in cells) per cell the camera moves; the
    // view height is about 2·tan(15°)·distance. Dust moves 1.6 to 2.4 times as much.
    let ground = 1.0 / max(0.536 * post.time.w, 1.0);
    let parallax = vec2<f32>(post.time.y, -post.time.z) * ground * mix(1.6, 2.4, near);
    let drift = vec2<f32>(0.010, -0.004) * t * (1.0 + near);
    let q = (p + parallax + drift) * scale;
    let cell = vec2<i32>(floor(q));
    var light = 0.0;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let c = cell + vec2<i32>(dx, dy);
            if (hash3(c, layer, 0u) > DUST_DENSITY) {
                continue;
            }
            let phase = 6.283 * hash3(c, layer, 3u);
            let centre = vec2<f32>(c) + 0.5
                + (vec2<f32>(hash3(c, layer, 1u), hash3(c, layer, 2u)) - 0.5) * 0.7
                + 0.12 * vec2<f32>(sin(t * 0.4 + phase), cos(t * 0.33 + phase));
            let radius = mix(0.07, 0.16, hash3(c, layer, 4u)) * mix(1.0, 1.4, near);
            // Out of focus: a disc of nearly even light with a soft rim (bokeh).
            let disc = 1.0 - smoothstep(radius * 0.55, radius, length(q - centre));
            let twinkle = 0.55 + 0.45 * sin(t * 1.1 + phase * 3.0);
            light += disc * twinkle;
        }
    }
    return light;
}

// ---------- Vignette and grain ----------

const VIGNETTE: f32 = 0.28;
const GRAIN: f32 = 0.025;

fn hash(p: vec2<u32>, frame: u32) -> f32 {
    var h = p.x * 0x8da6b343u ^ p.y * 0xd8163841u ^ frame * 0xcb1ab31fu;
    h = (h ^ (h >> 13u)) * 0x5bd1e995u;
    h = h ^ (h >> 15u);
    return f32(h) / 4294967295.0;
}

@fragment
fn fs_post(in: FullScreen) -> @location(0) vec4<f32> {
    let size = vec2<f32>(textureDimensions(scene));
    let texel = 1.0 / size;
    let uv = in.uv;

    let away = max(abs(uv.y - FOCUS_CENTRE) - FOCUS_HALF_WIDTH, 0.0);
    let blur = smoothstep(0.0, FOCUS_FADE, away) * MAX_BLUR * size.y / 1080.0;
    var color = disc_blur(uv, texel * blur);
    color += bloom(uv, texel, size.y);
    color = grade(color);

    // Foreground dust, stronger away from the sharp band (where it is "closer").
    let screen = (uv - 0.5) * vec2<f32>(size.x / size.y, 1.0);
    var specks = 0.0;
    for (var layer = 0; layer < DUST_LAYERS; layer++) {
        specks += dust(screen, layer, post.time.x);
    }
    let foreground = 0.5 + 0.5 * smoothstep(0.0, FOCUS_FADE, away);
    // Dust shines only in daylight: at night, nothing lights it.
    let daylight = 1.0 - 0.95 * post.night.x;
    color += DUST_COLOR * specks * DUST_STRENGTH * foreground * daylight;

    // Vignette: darker towards the corners (aspect corrected), guiding the eye to the centre.
    let centred = (uv - 0.5) * vec2<f32>(size.x / size.y, 1.0);
    // A weary naturalist sees the world dimmer at the edges and duller.
    let weary = post.night.y;
    color *= 1.0 - (VIGNETTE + 0.4 * weary) * smoothstep(0.35 - 0.2 * weary, 1.0, length(centred) * 1.2);
    color = mix(vec3<f32>(luminance(color)), color, 1.0 - 0.45 * weary);

    // Grain: fine noise changing 24 times a second; it also hides banding in the haze.
    let frame = u32(post.time.x * 24.0);
    let noise = hash(vec2<u32>(in.position.xy), frame) - 0.5;
    color += noise * GRAIN * sqrt(max(color, vec3<f32>(0.0)));

    return vec4<f32>(color, 1.0);
}
