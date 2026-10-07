// Voxel volumes. Two fragment entry points share the same vertex stage and lighting:
// - `fs_main`, opaque (ground): inert base colour blended with the "life" overlay (e.g. a wet
//   film), ambient occlusion;
// - `fs_water`, transparent: colour by depth, foam where the current is fast, animated ripples,
//   sun glint, opacity growing with depth.
// Particles (`vs_particle`, `fs_particle`) are small instanced cubes using only group 0.
// Lighting: warm sun, cool sky light in the shadows, haze with distance.

// ---------- Group 0: data shared by the whole frame ----------

// Rust side: `CameraUniform` in src/camera.rs.
struct Camera {
    view_proj: mat4x4<f32>,  // offset 0, size 64
    eye: vec4<f32>,          // offset 64: eye position, w = distance to the target
    time: vec4<f32>,         // offset 80: x = seconds, y = mist, z = wetness, w = mist floor
    light_view_proj: mat4x4<f32>,  // offset 96, size 64: world → sun's shadow map
    world: vec4<f32>,        // offset 160: size of the world in x, z (0: it has edges); zw:
                             // the point looked at
    inverse_view_proj: mat4x4<f32>,  // offset 176, size 64: clip space → world
    focus: vec4<f32>,        // offset 240: feet of the one followed (xyz), w = reach
}

// The world closes on itself (a planet): a point is drawn at its copy nearest the point the
// camera looks at (not the eye, which may stand far off), a whole number of worlds away. The
// vertices of a face move together (they are much closer to one another than half a world),
// so nothing tears where one looks.
//
// The shift is taken from an anchor shared by every vertex of a thing (a face's cell, a plant's
// foot, a part's origin, a particle's centre): a thing moves as a whole, and none is torn in
// two when it lies half a world away.
fn wrap_shift(anchor: vec3<f32>) -> vec3<f32> {
    if (camera.world.x <= 0.0) {
        return vec3<f32>(0.0);
    }
    return vec3<f32>(
        camera.world.x * round((camera.world.z - anchor.x) / camera.world.x),
        0.0,
        camera.world.y * round((camera.world.w - anchor.z) / camera.world.y),
    );
}

// The anchor of a volume vertex: the cell its face belongs to. A vertex that carries its
// material (a micro-voxel of a dug cell) has no cell packed in: it anchors at its own position.
// The corners of such a face lie a quarter of a cell apart, far closer than half a world.
fn cell_anchor(cell: u32, position: vec3<f32>) -> vec3<f32> {
    if ((cell & DIRECT_MATERIAL) != 0u) {
        return position + volume.origin.xyz;
    }
    return vec3<f32>(unpack_cell(cell)) + volume.origin.xyz;
}

// Rust side: `AtmosphereUniform` in src/palette.rs. `w` unused unless stated.
struct Atmosphere {
    sun_direction: vec4<f32>,  // offset 0, towards the sun, normalised; w = moon's lit share
    sun_color: vec4<f32>,      // offset 16
    sky_color: vec4<f32>,      // offset 32, w = wind towards x
    ground_color: vec4<f32>,   // offset 48, w = wind towards z
    fog_color: vec4<f32>,      // offset 64, w = grass dried by the season (0 to 1)
    fog: vec4<f32>,            // offset 80: start, end, max, w = stars (0 to 1)
    season: vec4<f32>,         // offset 96: leaves turned, leaves fallen, snow, ice (0 to 1)
}

// Rust side: `MaterialsUniform` in src/palette.rs. Indexed by material id.
struct Materials {
    colors: array<vec4<f32>, 64>,  // offset 0, size 1024 (rgb + unused w)
}

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> atmosphere: Atmosphere;
@group(0) @binding(2) var<uniform> materials: Materials;
// Depth of the first lit surface seen from the sun, per texel (see `vs_shadow`).
@group(0) @binding(3) var shadow_map: texture_depth_2d;
// Compares a depth with the map instead of returning it: 1 = lit, 0 = in shadow, and with
// linear filtering a blend of the 4 nearest comparisons.
@group(0) @binding(4) var shadow_sampler: sampler_comparison;

// Rust side: `MarksUniform` in src/marks.rs.
struct Marks {
    ripples: array<vec4<f32>, 16>,  // offset 0: x, z, start time, strength (0 = none)
    prints: array<vec4<f32>, 48>,   // offset 256: x, z, facing, start time (< 0 = none)
}
@group(0) @binding(5) var<uniform> marks: Marks;

// ---------- Group 1: the volume being drawn (terrain, water…) ----------

// Rust side: `VolumeUniform` in src/volume.rs.
struct Volume {
    origin: vec4<f32>,                // offset 0: world position of cell (0,0,0), w unused
    base_stops: array<vec4<f32>, 5>,  // offset 16, size 80 (rgb + unused w)
    life_stops: array<vec4<f32>, 5>,  // offset 96, size 80
    base_range: vec4<f32>,            // offset 176: min, max, materials (1 = yes), unused
    life_range: vec4<f32>,            // offset 192: fade start, fade end, colour min, colour max
}

// One f32 per cell each, same layout as `sim::grid::Field3`.
@group(1) @binding(0) var base_field: texture_3d<f32>;
@group(1) @binding(1) var<uniform> volume: Volume;
@group(1) @binding(2) var life_field: texture_3d<f32>;

// ---------- Vertex stage ----------

// Rust side: `Vertex` in src/mesh.rs.
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) cell: u32,  // x | y << 10 | z << 20
    @location(3) ao: f32,    // 1 = open, 0 = buried
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // Integers cannot be interpolated between vertices: `flat` passes the value of one vertex
    // unchanged. All four vertices of a face share the same cell anyway.
    @location(2) @interpolate(flat) cell: u32,
    // Interpolated across the face: this is what makes creases fade smoothly into shadow.
    @location(3) ao: f32,
    // Colour multiplier of foliage (plant models: each tree its own shade; 1 elsewhere).
    @location(4) tint: vec3<f32>,
    // 1 where it may be cut away when it hides the one followed (ground, plants), 0 for the
    // characters themselves.
    @location(5) cuttable: f32,
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Mesh positions are in grid cells; the volume's origin places them in the world.
    let world_position = in.position + volume.origin.xyz + wrap_shift(cell_anchor(in.cell, in.position));
    out.clip_position = camera.view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    out.normal = in.normal;
    out.cell = in.cell;
    out.ao = in.ao;
    out.tint = vec3<f32>(1.0);
    out.cuttable = 1.0;
    return out;
}

// ---------- Shared helpers ----------

// Material ids (Rust side: `world::Material`).
const MATERIAL_LEAVES: u32 = 11u;
const MATERIAL_PINE_NEEDLES: u32 = 12u;
const MATERIAL_TALL_GRASS: u32 = 16u;
const MATERIAL_FERN: u32 = 21u;
const MATERIAL_BIRCH_LEAVES: u32 = 25u;
const MATERIAL_PALM_LEAVES: u32 = 27u;
const MATERIAL_WILLOW_LEAVES: u32 = 30u;

// Green parts of plants: their shade varies from plant to plant (`tint`).
fn is_foliage(id: u32) -> bool {
    return id == MATERIAL_LEAVES || id == MATERIAL_PINE_NEEDLES || id == MATERIAL_TALL_GRASS
        || id == MATERIAL_FERN || id == MATERIAL_BIRCH_LEAVES || id == MATERIAL_PALM_LEAVES
        || id == MATERIAL_WILLOW_LEAVES;
}

const MATERIAL_GRASS: u32 = 1u;
const MATERIAL_FOREST_FLOOR: u32 = 2u;

// Leaves that turn and fall in autumn (the conifers and palms keep theirs).
fn is_deciduous(id: u32) -> bool {
    return id == MATERIAL_LEAVES || id == MATERIAL_BIRCH_LEAVES || id == MATERIAL_WILLOW_LEAVES;
}

// Autumn colour of a leaf voxel, `grain` its own random number: birch gold, willow yellow,
// the broadleaf from yellow through orange to russet.
fn autumn_leaf(id: u32, grain: f32) -> vec3<f32> {
    if (id == MATERIAL_BIRCH_LEAVES) {
        return mix(vec3<f32>(0.78, 0.55, 0.06), vec3<f32>(0.9, 0.7, 0.12), grain);
    }
    if (id == MATERIAL_WILLOW_LEAVES) {
        return mix(vec3<f32>(0.62, 0.6, 0.1), vec3<f32>(0.8, 0.68, 0.16), grain);
    }
    let warm = mix(vec3<f32>(0.85, 0.55, 0.06), vec3<f32>(0.7, 0.2, 0.04), grain);
    return mix(warm, vec3<f32>(0.35, 0.16, 0.06), smoothstep(0.75, 1.0, grain));
}

// Wind on plants, after "Vegetation Procedural Animation and Shading in Crysis" (GPU Gems 3,
// ch. 16): a main bending of the whole plant along the wind, and a detail bending of its crown.
//
// The wind turns over the hours (game/wind.rs): the direction it blows towards, unit.
fn wind_direction() -> vec3<f32> {
    return vec3<f32>(atmosphere.sky_color.w, 0.0, atmosphere.ground_color.w);
}
// Clouds drift with the wind aloft, which keeps its direction.
const CLOUD_DRIFT: vec2<f32> = vec2<f32>(0.89, 0.45);
// Height (cells) used to normalise the bending: a plant this tall bends "fully".
const PLANT_HEIGHT: f32 = 10.0;
// Main bending: how far the top of a `PLANT_HEIGHT` plant moves at full wind, in cells.
const MAIN_BEND: f32 = 0.6;
// Detail bending of the crown, in cells: up and down, and side to side.
const DETAIL_UP: f32 = 0.06;
const DETAIL_SIDE: f32 = 0.04;

// Smooth triangle wave in [0, 1], period 1: cheaper than a sine and, smoothed with a cubic, as
// soft. Sums of a few of them at unrelated frequencies never repeat visibly.
fn smooth_triangle(x: f32) -> f32 {
    let t = abs(fract(x + 0.5) * 2.0 - 1.0);
    return t * t * (3.0 - 2.0 * t);
}

// Wind strength in [0, 1] at a plant: slow gusts (periods of 5 to 15 s) travelling with the
// wind across the land, so trees bend one after the other, plus each plant's own small lag.
fn wind_strength(base: vec3<f32>, t: f32, phase: f32) -> f32 {
    let along = dot(base, wind_direction()) * 0.012;
    let gusts = 0.5 * smooth_triangle(t * 0.07 - along)
        + 0.3 * smooth_triangle(t * 0.13 - along * 1.7 + 0.31)
        + 0.2 * smooth_triangle(t * 0.21 + phase * 0.05);
    return clamp(gusts, 0.0, 1.0);
}

// Moves a model vertex `local` (relative to the plant's base, already sized and oriented).
// `base`: where the plant stands; `phase`: its own rhythm; `flexibility`: 0 (rigid, a
// cactus) to 1 (a broadleaf tree). Only depends on the position: vertices at the same place
// move the same way, whatever their material, so the mesh never tears.
fn wind(local: vec3<f32>, base: vec3<f32>, phase: f32, flexibility: f32) -> vec3<f32> {
    if (flexibility <= 0.0 || local.y <= 0.0) {
        return base + local;
    }
    let t = camera.time.x;
    let strength = wind_strength(base, t, phase);

    // Main bending. Bend factor from the normalised height (Crysis): 0 at the base, growing
    // faster than linearly, so the trunk barely moves and the crown sways.
    let h = local.y / PLANT_HEIGHT;
    var bend = h * 0.1 + 1.0;
    bend = bend * bend;
    bend = bend * bend - bend;
    // The plant leans with the wind and breathes around that lean, never past the vertical.
    let lean = strength * (0.75 + 0.25 * smooth_triangle(t * 0.31 + phase));
    let towards = wind_direction();
    // Horizontal direction across the wind, for side-to-side motion.
    let across = vec3<f32>(-towards.z, 0.0, towards.x);
    var moved = local + towards * (MAIN_BEND / 0.254 * flexibility * lean * bend);
    // Keep the distance to the base: the top moves on an arc (slightly down when it bends)
    // instead of sliding sideways and stretching the plant.
    moved = normalize(moved) * length(local);

    // Detail bending of the crown (upper part of the plant only). The phase varies slowly
    // across the crown, so a clump of leaves moves as one ("cohesive unit") while clumps far
    // apart move differently.
    let crown = smoothstep(0.3, 0.8, h) * flexibility * (0.3 + 0.7 * strength);
    let clump = phase + dot(base + local, vec3<f32>(0.31, 0.23, 0.27));
    let side = smooth_triangle(t * 1.975 + clump) + smooth_triangle(t * 0.793 + clump * 1.3) - 1.0;
    let up = smooth_triangle(t * 0.375 + clump * 0.7) + smooth_triangle(t * 0.193 + clump * 1.7) - 1.0;
    moved += across * (DETAIL_SIDE * crown * side);
    moved.y += DETAIL_UP * crown * up;
    return base + moved;
}

// Flag bit of a vertex's `cell`: the material is carried by the vertex itself (id in bits 0–7,
// brightness variation in bits 8–15), not read from the base field. Rust side:
// `mesh::DIRECT_MATERIAL`. Used by plant models.
const DIRECT_MATERIAL: u32 = 0x80000000u;

// Value of the base field for a vertex's cell: from the texture, or carried by the vertex
// (as id + variation, like a material field).
fn base_value(cell: u32) -> f32 {
    if ((cell & DIRECT_MATERIAL) != 0u) {
        return f32(cell & 0xffu) + f32((cell >> 8u) & 0xffu) / 256.0;
    }
    return textureLoad(base_field, unpack_cell(cell), 0).r;
}

fn unpack_cell(packed: u32) -> vec3<u32> {
    return vec3<u32>(packed & 1023u, (packed >> 10u) & 1023u, packed >> 20u);
}

// Position of `value` in [min, max], clamped to [0, 1].
fn unit(value: f32, min_value: f32, max_value: f32) -> f32 {
    return clamp((value - min_value) / (max_value - min_value), 0.0, 1.0);
}

// Piecewise-linear interpolation between the stops of a palette, `t` in [0, 1].
// Uniform arrays cannot be passed to functions, hence one function per palette.
fn base_palette(t: f32) -> vec3<f32> {
    let x = t * 4.0;  // 4 = number of stops - 1
    let i = min(u32(x), 3u);
    return mix(volume.base_stops[i].rgb, volume.base_stops[i + 1u].rgb, x - f32(i));
}

fn life_palette(t: f32) -> vec3<f32> {
    let x = t * 4.0;
    let i = min(u32(x), 3u);
    return mix(volume.life_stops[i].rgb, volume.life_stops[i + 1u].rgb, x - f32(i));
}

// Light left in the most occluded corner: never pitch black, shadows stay coloured.
const AO_FLOOR: f32 = 0.35;
// Size of a shadow map texel in texture coordinates (map of 2048×2048, see renderer.rs).
const SHADOW_TEXEL: f32 = 1.0 / 2048.0;
// The point is pushed this far along its normal before the lookup, so a surface does not
// shadow itself because of the map's limited precision ("shadow acne").
const SHADOW_NORMAL_OFFSET: f32 = 0.2;

// How much of the sun reaches `p`, in [0, 1]: 0 when something stands between it and the sun.
// 3×3 comparisons around the point, averaged (percentage-closer filtering): soft edges
// instead of a staircase. `textureSampleCompareLevel` (not `…Compare`) may be called anywhere,
// even under non-uniform branches.
fn sun_visibility(p: vec3<f32>, n: vec3<f32>) -> f32 {
    let clip = camera.light_view_proj * vec4<f32>(p + n * SHADOW_NORMAL_OFFSET, 1.0);
    let ndc = clip.xyz / clip.w;
    // Clip space y goes up, texture v goes down.
    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let clouds = cloud_light(p);
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || ndc.z > 1.0) {
        return clouds;
    }
    var lit = 0.0;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let offset = vec2<f32>(f32(dx), f32(dy)) * SHADOW_TEXEL;
            lit += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + offset, ndc.z);
        }
    }
    return lit / 9.0 * clouds;
}

// ---------- Cloud shadows ----------

// Clouds are not drawn (the camera looks down), only their shadows: patches of shade drifting
// over the land with the wind. A cloud layer of fractal noise at `CLOUD_HEIGHT` is looked up
// where the ray from the point towards the sun crosses it, so shadows fall the right way and
// slide across slopes.
const CLOUD_HEIGHT: f32 = 60.0;
// Size of the clouds: about 1 / CLOUD_SCALE cells across.
const CLOUD_SCALE: f32 = 0.03;
// Drift, cells per second (along the wind).
const CLOUD_SPEED: f32 = 1.5;
// Share of the sky covered, and how much sunlight a cloud lets through.
const CLOUD_COVER: f32 = 0.6;
const CLOUD_LIGHT: f32 = 0.3;

fn hash2(c: vec2<i32>) -> f32 {
    var h = bitcast<u32>(c.x) * 0x8da6b343u ^ bitcast<u32>(c.y) * 0xd8163841u;
    h = (h ^ (h >> 13u)) * 0x5bd1e995u;
    h = h ^ (h >> 15u);
    return f32(h) / 4294967295.0;
}

// Value noise: random values at integer points, smoothly interpolated between them.
fn value_noise(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<i32>(1, 0));
    let c = hash2(i + vec2<i32>(0, 1));
    let d = hash2(i + vec2<i32>(1, 1));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// 1 in full sun, `CLOUD_LIGHT` under the thick of a cloud.
fn cloud_light(p: vec3<f32>) -> f32 {
    let sun = atmosphere.sun_direction.xyz;
    let towards = p + sun * ((CLOUD_HEIGHT - p.y) / max(sun.y, 0.1));
    var q = (towards.xz - CLOUD_DRIFT * CLOUD_SPEED * camera.time.x) * CLOUD_SCALE;
    // Fractal noise: three octaves, each twice finer and half as strong.
    var n = 0.0;
    var amplitude = 0.5;
    for (var k = 0; k < 3; k++) {
        n += value_noise(q) * amplitude;
        q = q * 2.03 + vec2<f32>(17.1, 9.3);
        amplitude *= 0.5;
    }
    let cloud = smoothstep(1.0 - CLOUD_COVER - 0.08, 1.0 - CLOUD_COVER + 0.12, n / 0.875);
    return mix(1.0, CLOUD_LIGHT, cloud);
}

// Sun (Lambert: light ∝ cos of the angle to the sun, times its visibility) plus hemispheric
// ambient (faces looking up see the cool sky, faces looking down the warm ground), the
// ambient dimmed by occlusion. In shadow only the sun is gone: shadows keep the sky's blue.
fn shade(albedo: vec3<f32>, n: vec3<f32>, ao: f32, sunlit: f32) -> vec3<f32> {
    let sun = atmosphere.sun_color.rgb * max(dot(n, atmosphere.sun_direction.xyz), 0.0) * sunlit;
    let ambient = mix(atmosphere.ground_color.rgb, atmosphere.sky_color.rgb, n.y * 0.5 + 0.5);
    return albedo * (sun + ambient) * mix(AO_FLOOR, 1.0, ao);
}

// Haze: grows with the distance beyond the camera target, so the far side of the scene fades
// into the background whatever the zoom.
fn haze(color: vec3<f32>, world_position: vec3<f32>) -> vec3<f32> {
    let depth = distance(world_position, camera.eye.xyz) - camera.eye.w;
    let fog = atmosphere.fog;
    let amount = clamp((depth - fog.x) / (fog.y - fog.x), 0.0, 1.0) * fog.z;
    var hazed = mix(color, atmosphere.fog_color.rgb, amount);
    // Ground mist: dense below the mist floor, thinning above it, so it pools in hollows and
    // over water; a slow drift breaks it into banks.
    let mist = camera.time.y;
    if (mist > 0.0) {
        let above = world_position.y - camera.time.w;
        let layer = exp(-max(above, 0.0) / 1.2);
        let drift = world_position.xz * 0.05 + vec2<f32>(camera.time.x * 0.02, camera.time.x * 0.013);
        let banks = 0.55 + 0.45 * value_noise(drift) + 0.25 * value_noise(drift * 2.7);
        let thickness = clamp(mist * layer * banks * 0.6, 0.0, 0.75);
        let mist_color = mix(atmosphere.fog_color.rgb, vec3<f32>(1.0), 0.25) * 1.05;
        hazed = mix(hazed, mist_color, thickness);
    }
    // Edge of what is drawn: the ground melts into the haze before it ends, as the sky meets
    // the land at the horizon.
    let reach = camera.focus.w;
    if (reach > 0.0) {
        let from_look = length(world_position.xz - camera.world.zw);
        hazed = mix(hazed, atmosphere.fog_color.rgb, smoothstep(reach * 0.6, reach * 0.95, from_look));
    }
    return hazed;
}

// ---------- Cut-away: nothing hides the one followed ----------

// What stands between the eye and the one followed, and rises above their feet (a tree, a
// hill), is not drawn inside a cone from the eye to them: a round window on the screen, this
// wide at their place (cells), with a dithered rim. Flat ground at their feet is never cut,
// nor what stands just in front of them, nor the shadows (the shadow pass has no fragments).
const CUT_RADIUS: f32 = 2.2;
const CUT_RIM: f32 = 0.5;
const CUT_SPARED: f32 = 1.0;
const CUT_ABOVE_FEET: f32 = 0.3;
// Close to the eye (a canopy it looks out of), all that rises above the feet goes, fading over
// a rim: within this many cells, or less on a short view.
const CUT_NEAR_EYE: f32 = 3.0;
const CUT_NEAR_RIM: f32 = 1.0;

// A number in [0, 1) that changes from pixel to pixel without visible pattern ("interleaved
// gradient noise"): it dithers the rim of the window into a soft edge.
fn pixel_noise(pixel: vec2<f32>) -> f32 {
    return fract(52.9829189 * fract(dot(pixel, vec2<f32>(0.06711056, 0.00583715))));
}

fn cut_away(p: vec3<f32>, pixel: vec2<f32>) -> bool {
    let feet = camera.focus.xyz;
    if (camera.focus.w <= 0.0 || p.y < feet.y + CUT_ABOVE_FEET) {
        return false;
    }
    let eye = camera.eye.xyz;
    let axis = feet + vec3<f32>(0.0, 1.0, 0.0) - eye;
    let span = length(axis);
    let along_axis = axis / span;
    let along = dot(p - eye, along_axis);
    if (along <= 0.0 || along > span - CUT_SPARED) {
        return false;
    }
    let noise = pixel_noise(pixel);
    if ((min(CUT_NEAR_EYE, span * 0.4) - along) / CUT_NEAR_RIM > noise) {
        return true;
    }
    // The cone narrows towards the eye: the window keeps its size on the screen.
    let scale = along / span;
    let off = length(p - eye - along_axis * along);
    let inside = (CUT_RADIUS * scale - off) / (CUT_RIM * scale);
    return inside > noise;
}

// ---------- Marks: footprints and ripples ----------

// Seconds a footprint lasts (fading over its last 40 %), as in game/src/traces.rs.
const PRINT_LIFE: f32 = 40.0;
// Prints drawn a little larger than a real foot, to stay readable from above.
const PRINT_SCALE: f32 = 2.0;
const MATERIAL_SAND: u32 = 4u;
const MATERIAL_DESERT_SAND: u32 = 5u;
const MATERIAL_SNOW: u32 = 8u;

// How much the ground at `p` is pressed by a footprint (x, the hollow) and pushed up around
// it (y, the rim). A sole: the heel and the ball of the foot, two ovals.
fn footprints(p: vec2<f32>) -> vec2<f32> {
    var press = 0.0;
    var rim = 0.0;
    for (var i = 0; i < 48; i++) {
        let m = marks.prints[i];
        let age = camera.time.x - m.w;
        if (m.w < 0.0 || age < 0.0) {
            continue;
        }
        let d = (p - m.xy) / PRINT_SCALE;
        if (dot(d, d) > 0.04) {
            continue;
        }
        let fade = 1.0 - smoothstep(PRINT_LIFE * 0.6, PRINT_LIFE, age);
        let forward = vec2<f32>(sin(m.z), cos(m.z));
        let side = vec2<f32>(forward.y, -forward.x);
        let q = vec2<f32>(dot(d, side), dot(d, forward));
        let heel = length((q - vec2<f32>(0.0, -0.07)) / vec2<f32>(0.045, 0.05));
        let ball = length((q - vec2<f32>(0.0, 0.05)) / vec2<f32>(0.055, 0.075));
        let sole = min(heel, ball);
        press = max(press, (1.0 - smoothstep(0.7, 1.0, sole)) * fade);
        rim = max(rim, smoothstep(0.85, 1.0, sole) * (1.0 - smoothstep(1.0, 1.35, sole)) * fade);
    }
    return vec2<f32>(press, rim);
}

// Ripples spreading on the water from where something touched it: a ring of small waves
// moving out at `RIPPLE_SPEED`, a few wavelengths wide, dying away. Returns the tilt of the
// surface (xy) and how much of a crest is there (z).
const RIPPLE_SPEED: f32 = 0.7;
const RIPPLE_LIFE: f32 = 3.0;
const RIPPLE_WIDTH: f32 = 0.28;
const RIPPLE_WAVENUMBER: f32 = 20.0;
const RIPPLE_TILT: f32 = 0.5;

fn ripple_rings(p: vec2<f32>) -> vec3<f32> {
    var tilt = vec2<f32>(0.0);
    var crest = 0.0;
    for (var i = 0; i < 16; i++) {
        let r = marks.ripples[i];
        let age = camera.time.x - r.z;
        if (r.w <= 0.0 || age < 0.0 || age > RIPPLE_LIFE) {
            continue;
        }
        let offset = p - r.xy;
        let d = length(offset);
        let x = d - (0.1 + RIPPLE_SPEED * age);
        // Wider and weaker as it spreads: the energy of the ring is shared over a longer
        // circle, so its height falls as 1/√d; damping fades it out over its life.
        let width = RIPPLE_WIDTH * (1.0 + 0.6 * age);
        let envelope = exp(-x * x / (width * width)) * (1.0 - age / RIPPLE_LIFE) * r.w
            / sqrt(1.0 + 2.0 * d);
        let phase = x * RIPPLE_WAVENUMBER / (1.0 + 0.5 * age);
        tilt += offset / max(d, 1.0e-3) * sin(phase) * envelope * RIPPLE_TILT;
        crest += max(cos(phase), 0.0) * envelope;
    }
    return vec3<f32>(tilt, crest);
}

// ---------- Opaque volumes ----------

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.cuttable > 0.5 && cut_away(in.world_position, in.clip_position.xy)) {
        discard;
    }
    // Albedo: the inert colour, covered by the overlay colour as the overlay value grows.
    let direct = (in.cell & DIRECT_MATERIAL) != 0u;
    let value = base_value(in.cell);
    var base: vec3<f32>;
    if (direct || volume.base_range.z > 0.5) {
        // Material id in the integer part, a small brightness variation in the fraction.
        let id = min(u32(value), 63u);
        base = materials.colors[id].rgb * (0.88 + 0.24 * fract(value));
    } else {
        base = base_palette(unit(value, volume.base_range.x, volume.base_range.y));
    }
    // Models have no overlay field.
    var life_value = 0.0;
    if (!direct) {
        life_value = textureLoad(life_field, unpack_cell(in.cell), 0).r;
    }
    let living = life_palette(unit(life_value, volume.life_range.z, volume.life_range.w));
    let coverage = smoothstep(volume.life_range.x, volume.life_range.y, life_value);
    var albedo = mix(base, living, coverage);
    // Each tree its own shade of foliage.
    let id = u32(value);
    if ((direct || volume.base_range.z > 0.5) && is_foliage(id)) {
        albedo *= in.tint;
    }
    let n = normalize(in.normal);

    // The season. Each voxel has its own random number (its brightness variation), so a
    // voxel turns, falls or whitens as a whole.
    let materials_drawn = direct || volume.base_range.z > 0.5;
    let grain = fract(value * 0.999 + 0.37);
    if (materials_drawn && is_deciduous(id)) {
        // Fallen: the voxel is gone, the earlier the lower its number.
        if (grain < atmosphere.season.y * 1.02) {
            discard;
        }
        albedo = mix(albedo, autumn_leaf(id, fract(grain * 7.13)), atmosphere.season.x);
    }
    if (materials_drawn && (id == MATERIAL_GRASS || id == MATERIAL_TALL_GRASS)) {
        let straw = vec3<f32>(0.62, 0.55, 0.3) * (0.85 + 0.3 * grain);
        albedo = mix(albedo, straw, atmosphere.fog_color.w * 0.85);
    }
    // Snow on what faces the sky: patchy while thin, a white sheet once deep.
    let snow_cover = atmosphere.season.z;
    if (snow_cover > 0.0 && materials_drawn && n.y > 0.5) {
        let patchy = smoothstep(grain - 0.15, grain + 0.15, snow_cover * 1.3);
        let snowy = patchy * smoothstep(0.5, 0.85, n.y);
        albedo = mix(albedo, vec3<f32>(0.9, 0.92, 0.95), snowy);
    }
    // Footprints in sand and snow: the hollow is darker, its rim a little lighter.
    if (!direct && volume.base_range.z > 0.5 && n.y > 0.5
        && (id == MATERIAL_SAND || id == MATERIAL_DESERT_SAND || id == MATERIAL_SNOW)) {
        let print = footprints(in.world_position.xz);
        albedo *= (1.0 - 0.4 * print.x) * (1.0 + 0.15 * print.y);
    }
    let sunlit = sun_visibility(in.world_position, n);
    // Wet ground and leaves after rain: darker (water fills the pores) and glossy on top.
    let wet = camera.time.z * smoothstep(0.3, 0.8, n.y);
    albedo *= 1.0 - 0.35 * wet;
    var color = shade(albedo, n, in.ao, sunlit);
    if (wet > 0.0) {
        let to_eye = normalize(camera.eye.xyz - in.world_position);
        let mirrored = reflect(-atmosphere.sun_direction.xyz, n);
        let sheen = pow(max(dot(mirrored, to_eye), 0.0), 24.0) * 0.5 + 0.06;
        color += atmosphere.sky_color.rgb * sheen * wet + atmosphere.sun_color.rgb * pow(max(dot(mirrored, to_eye), 0.0), 64.0) * wet * sunlit;
    }
    return vec4<f32>(haze(color, in.world_position), 1.0);
}

// 1 on a face turned up, 0 on the sides.
fn top_face_of(normal: vec3<f32>) -> f32 {
    return step(0.5, normal.y);
}

// ---------- Transparent water ----------

// Opacity of the shallowest and of the deepest water.
const WATER_ALPHA_MIN: f32 = 0.6;
const WATER_ALPHA_MAX: f32 = 0.95;
// Strength of the ripples (tilt of the surface normal) and of the sun glint.
const RIPPLE_STRENGTH: f32 = 0.08;
const GLINT_STRENGTH: f32 = 0.6;
const GLINT_SHARPNESS: f32 = 48.0;

// Small moving tilt of the water surface: a few crossing sine waves, a cheap stand-in for
// waves. Only the lighting moves, the geometry stays flat.
fn ripple(p: vec2<f32>, t: f32) -> vec2<f32> {
    let a = sin(p.x * 1.7 + t * 1.3) + sin((p.x + p.y) * 2.3 - t * 1.9);
    let b = cos(p.y * 1.9 - t * 1.1) + cos((p.x - p.y) * 2.7 + t * 1.7);
    return vec2<f32>(a, b) * RIPPLE_STRENGTH;
}

// Night sky seen in the water: stars, the Milky Way, the moon. The sky dome is flattened onto
// a plane (`sky_plane`) and cut into a grid; some cells hold a star at a random place. Stars
// are sized in screen pixels (`pixel`: size of a pixel on that plane, from the screen
// derivatives), so they stay crisp points at any distance: a sharp core of 1 to 3 pixels,
// anti-aliased over one pixel, and for the brightest, four thin twinkling rays.
const STAR_GRID: f32 = 12.0;
const STAR_SHARE: f32 = 0.75;
// Direction the Milky Way's band is perpendicular to.
const GALAXY_AXIS: vec3<f32> = vec3<f32>(0.62, 0.35, -0.7);
// Apparent radius of the moon (cosine of the angle), and of its glow.
const MOON_SIZE: f32 = 0.9985;
const MOON_GLOW: f32 = 0.97;

fn sky_plane(d: vec3<f32>) -> vec2<f32> {
    return d.xz / (max(d.y, 0.02) + 0.4) * STAR_GRID;
}

fn star_color(h: f32) -> vec3<f32> {
    if (h < 0.25) {
        return vec3<f32>(0.72, 0.82, 1.0);  // hot, bluish
    } else if (h > 0.85) {
        return vec3<f32>(1.0, 0.8, 0.58);   // cool, amber
    }
    return vec3<f32>(1.0, 0.97, 0.92);
}

fn night_sky(d: vec3<f32>, pixel: f32) -> vec3<f32> {
    if (d.y <= 0.02) {
        return vec3<f32>(0.0);
    }
    let p = sky_plane(d);
    let cell = vec2<i32>(floor(p));
    var light = vec3<f32>(0.0);
    for (var oy = -1; oy <= 1; oy++) {
        for (var ox = -1; ox <= 1; ox++) {
            let c = cell + vec2<i32>(ox, oy);
            if (hash2(c + vec2<i32>(311, 97)) > STAR_SHARE) {
                continue;
            }
            let centre = vec2<f32>(c) + 0.15 + 0.7 * vec2<f32>(hash2(c), hash2(c + vec2<i32>(7, 3)));
            // Few bright stars, many faint ones.
            let size = pow(hash2(c + vec2<i32>(13, 51)), 2.5);
            let q = (p - centre) / pixel;  // in pixels
            let r = length(q);
            let radius = 1.2 + 2.6 * size;
            let core = 1.0 - smoothstep(radius - 0.5, radius + 0.5, r);
            // Rays along the axes: thin (about a pixel), longer for brighter stars.
            let reach = 3.0 + 12.0 * size;
            let ray_x = exp(-q.y * q.y / 0.6) * max(1.0 - abs(q.x) / reach, 0.0);
            let ray_y = exp(-q.x * q.x / 0.6) * max(1.0 - abs(q.y) / reach, 0.0);
            let rays = (ray_x + ray_y) * smoothstep(0.3, 0.75, size) * 0.8;
            let twinkle = 0.7 + 0.3 * sin(camera.time.x * (0.8 + 1.6 * hash2(c + vec2<i32>(5, 9))) + 40.0 * hash2(c));
            let brightness = (0.5 + 2.6 * size) * twinkle;
            light += star_color(hash2(c + vec2<i32>(17, 29))) * (core + rays) * brightness;
        }
    }
    // A finer layer of many faint stars: in a real sky, faint stars far outnumber bright ones.
    let fine = p * 3.0;
    let fine_cell = vec2<i32>(floor(fine));
    for (var oy = -1; oy <= 1; oy++) {
        for (var ox = -1; ox <= 1; ox++) {
            let c = fine_cell + vec2<i32>(ox, oy) + vec2<i32>(9001, 4007);
            if (hash2(c + vec2<i32>(311, 97)) > 0.5) {
                continue;
            }
            let centre = vec2<f32>(c - vec2<i32>(9001, 4007)) + 0.15 + 0.7 * vec2<f32>(hash2(c), hash2(c + vec2<i32>(7, 3)));
            let r = length((fine - centre) / (pixel * 3.0));
            let dot_ = 1.0 - smoothstep(0.4, 1.4, r);
            let twinkle = 0.6 + 0.4 * sin(camera.time.x * (1.0 + 2.0 * hash2(c + vec2<i32>(5, 9))) + 40.0 * hash2(c));
            light += star_color(hash2(c + vec2<i32>(17, 29))) * dot_ * (0.15 + 0.35 * hash2(c + vec2<i32>(13, 51))) * twinkle;
        }
    }
    // Milky Way: a faint, uneven band of light across the sky.
    let band = exp(-pow(dot(d, normalize(GALAXY_AXIS)), 2.0) / 0.03);
    let clouds = value_noise(p * 0.35) * 0.6 + value_noise(p * 0.9) * 0.4;
    light += vec3<f32>(0.55, 0.6, 0.85) * band * clouds * 0.12;
    // Moon: at night the light direction is the moon's.
    let facing = dot(d, atmosphere.sun_direction.xyz);
    let disc = smoothstep(MOON_SIZE - 0.0004, MOON_SIZE, facing);
    let glow = pow(smoothstep(MOON_GLOW, 1.0, facing), 2.0) * 0.3;
    light += vec3<f32>(0.95, 0.95, 0.88) * (disc * 3.0 + glow) * atmosphere.sun_direction.w;
    return light;
}

@fragment
fn fs_water(in: VertexOutput) -> @location(0) vec4<f32> {
    // Base field: water depth. Life field: speed of the current, shown as foam.
    let cell = unpack_cell(in.cell);
    let depth_t = unit(textureLoad(base_field, cell, 0).r, volume.base_range.x, volume.base_range.y);
    let speed = textureLoad(life_field, cell, 0).r;
    let foam = smoothstep(volume.life_range.x, volume.life_range.y, speed);
    let foam_color = life_palette(unit(speed, volume.life_range.z, volume.life_range.w));
    let albedo = mix(base_palette(depth_t), foam_color, foam);

    var n = normalize(in.normal);
    let sunlit = sun_visibility(in.world_position, n);
    var crest = 0.0;
    if (n.y > 0.5) {
        var tilt = ripple(in.world_position.xz, camera.time.x);
        let rings = ripple_rings(in.world_position.xz);
        tilt += rings.xy;
        crest = rings.z;
        n = normalize(vec3<f32>(tilt.x, 1.0, tilt.y));
    }
    var color = shade(albedo, n, 1.0, sunlit);
    // Ice: an opaque, pale, still surface (snow on it once it holds).
    let ice = atmosphere.season.w * top_face_of(in.normal);
    if (ice > 0.0) {
        let frozen = mix(vec3<f32>(0.62, 0.74, 0.82), vec3<f32>(0.9, 0.92, 0.95), atmosphere.season.z);
        color = mix(color, shade(frozen, normalize(in.normal), 1.0, sunlit), ice);
    }

    // Mirror: the surface reflects the sky, more at grazing angles (Fresnel), and at night the
    // stars. The reflected direction picks a star in a grid laid over the sky.
    let to_eye = normalize(camera.eye.xyz - in.world_position);
    let fresnel = 0.15 + 0.85 * pow(1.0 - max(dot(n, to_eye), 0.0), 3.0);
    let calm = 1.0 - foam;
    color = mix(color, atmosphere.sky_color.rgb, fresnel * 0.5 * calm);
    // The sky is mirrored by a still surface, with only a slow shiver: the ripples that move
    // the light would shatter the stars into arcs.
    let shiver = 0.004 * vec2<f32>(
        sin(in.world_position.z * 0.7 + camera.time.x * 0.9),
        cos(in.world_position.x * 0.6 + camera.time.x * 0.7),
    );
    let mirrored_sky = reflect(-to_eye, normalize(vec3<f32>(shiver.x, 1.0, shiver.y)));
    // Screen derivatives must be taken where every pixel of a group runs the same code: here,
    // outside any branch.
    let sky_p = sky_plane(mirrored_sky);
    let pixel = max(max(length(dpdx(sky_p)), length(dpdy(sky_p))), 1.0e-4);
    let top_face = step(0.5, in.normal.y);
    // Seen at a grazing angle (from low), the stars low in the sky are dimmed, as above.
    let low_sky = smoothstep(0.02, 0.35, mirrored_sky.y);
    color += night_sky(mirrored_sky, pixel) * atmosphere.fog.w * calm * top_face * (0.5 + 0.5 * fresnel) * low_sky;

    // Glint: the sun (or moon) mirrored by the surface, seen when the reflection points at
    // the eye.
    let mirrored = reflect(-atmosphere.sun_direction.xyz, n);
    let glint = pow(max(dot(mirrored, to_eye), 0.0), GLINT_SHARPNESS) * GLINT_STRENGTH;
    color += atmosphere.sun_color.rgb * glint * (1.0 - foam) * sunlit;

    // Crests of the rings catch the light of the sky.
    color += atmosphere.sky_color.rgb * crest * 0.5;

    // Shallow water lets the ground show through; deep water, foam and ice hide it.
    let alpha = clamp(mix(WATER_ALPHA_MIN, WATER_ALPHA_MAX, depth_t) + 0.4 * foam + ice, 0.0, 1.0);
    return vec4<f32>(haze(color, in.world_position), alpha);
}

// ---------- Sky: the background, seen when the view is low ----------

// Brightness of the stars seen directly, against their reflections.
const STARS_SEEN: f32 = 0.35;

// No vertex buffer: the vertex shader makes the three corners of a triangle large enough to
// cover the whole screen from the vertex index (0, 1, 2) alone.
struct SkyOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
}

@vertex
fn vs_sky(@builtin(vertex_index) index: u32) -> SkyOutput {
    // (−1, −1), (3, −1), (−1, 3): the screen square [−1, 1]² lies inside.
    let corner = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u)) * 2.0 - 1.0;
    var out: SkyOutput;
    out.clip_position = vec4<f32>(corner, 1.0, 1.0);
    out.ndc = corner;
    return out;
}

@fragment
fn fs_sky(in: SkyOutput) -> @location(0) vec4<f32> {
    // The direction this pixel looks along: its point on the far plane, seen from the eye.
    let far = camera.inverse_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let d = normalize(far.xyz / far.w - camera.eye.xyz);
    // The haze at the horizon (where the ground fades into it), the sky's colour above.
    let up = max(d.y, 0.0);
    var color = mix(atmosphere.fog_color.rgb, atmosphere.sky_color.rgb, smoothstep(0.0, 0.5, pow(up, 0.8)));
    // By day, the air glows around the sun, more at the horizon.
    let daylight = smoothstep(0.3, 0.8, dot(atmosphere.sun_color.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)));
    let towards_sun = max(dot(d, atmosphere.sun_direction.xyz), 0.0);
    color += atmosphere.sun_color.rgb * pow(towards_sun, 6.0) * (0.35 - 0.2 * up) * daylight;
    // At night, the stars and the moon (derivatives taken outside any branch). Seen directly
    // they are fainter than their glitter on the water, and dimmed near the horizon, where
    // their light crosses the most air.
    let sky_p = sky_plane(d);
    let pixel = max(max(length(dpdx(sky_p)), length(dpdy(sky_p))), 1.0e-4);
    let stars = night_sky(d, pixel) * atmosphere.fog.w * STARS_SEEN * smoothstep(0.02, 0.35, d.y);
    return vec4<f32>(color + stars, 1.0);
}

// ---------- Particles: one cube mesh drawn once per instance ----------

// Rust side: `ParticleInstance` in src/particles.rs (per instance) and the cube vertices.
struct ParticleInput {
    @location(0) corner: vec3<f32>,   // cube vertex, in [-0.5, 0.5]³
    @location(1) normal: vec3<f32>,
    @location(2) centre_size: vec4<f32>,  // instance: centre xyz, edge length w
    @location(3) color: vec4<f32>,        // instance: linear rgb, a = glow
}

struct ParticleOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
    @location(3) glow: f32,
}

@vertex
fn vs_particle(in: ParticleInput) -> ParticleOutput {
    var out: ParticleOutput;
    let world_position = in.centre_size.xyz + wrap_shift(in.centre_size.xyz)
        + in.corner * in.centre_size.w;
    out.clip_position = camera.view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    out.normal = in.normal;
    out.color = in.color.rgb;
    out.glow = in.color.a;
    return out;
}

// Brightness added to a fully glowing particle in the sun (see `ParticleInstance::color`).
const MOTE_GLOW: f32 = 2.2;

@fragment
fn fs_particle(in: ParticleOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let sunlit = sun_visibility(in.world_position, n);
    // Specks of dust and pollen catch the sun and shine: lit, they get brighter than white,
    // and the bloom of the post pass spreads them into small soft sparks.
    // Positive glow: lit by the sun (dust). Negative: shines by itself (fireflies).
    // Dust shines in sunlight, not moonlight: weighted by the light's strength.
    let daylight = smoothstep(0.3, 0.8, dot(atmosphere.sun_color.rgb, vec3<f32>(0.2126, 0.7152, 0.0722)));
    let glow = in.color * (max(in.glow, 0.0) * sunlit * daylight * MOTE_GLOW + max(-in.glow, 0.0));
    let color = shade(in.color, n, 1.0, sunlit) + glow;
    return vec4<f32>(haze(color, in.world_position), 1.0);
}

// ---------- Shadow pass: depth of the scene seen from the sun ----------

// Only the position matters: the depth buffer of this pass is the shadow map.
@vertex
fn vs_shadow(in: VertexInput) -> @builtin(position) vec4<f32> {
    let p = in.position + volume.origin.xyz + wrap_shift(cell_anchor(in.cell, in.position));
    return camera.light_view_proj * vec4<f32>(p, 1.0);
}

// ---------- Models: one mesh drawn at many places (plants) ----------

// Rust side: `ModelInstance` in src/models.rs (per-instance attributes).
struct ModelInstance {
    @location(4) position_turns: vec4<f32>,  // where the model stands (xyz), quarter turns (w)
    @location(5) scale_mirror: vec4<f32>,    // size (x), mirrored (y: 1 or 0), bend (zw)
    @location(6) tint: vec4<f32>,            // foliage colour multiplier (rgb), flexibility (a)
}

// Orients a model-space vector like `world::vegetation::orient`: mirrored (x ↦ −x) first if
// asked, then turned by quarter turns around the vertical axis.
fn orient(v: vec3<f32>, turns: f32, mirrored: f32) -> vec3<f32> {
    var w = v;
    if (mirrored > 0.5) {
        w.x = -w.x;
    }
    let k = u32(turns + 0.5) % 4u;
    if (k == 1u) {
        return vec3<f32>(-w.z, w.y, w.x);
    } else if (k == 2u) {
        return vec3<f32>(-w.x, w.y, -w.z);
    } else if (k == 3u) {
        return vec3<f32>(w.z, w.y, -w.x);
    }
    return w;
}

// A number in [0, 2π) that differs from plant to plant: its own rhythm in the wind.
fn plant_phase(position: vec3<f32>) -> f32 {
    return 6.2831853 * fract(sin(dot(position.xz, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

// Position of a model vertex in the world: sized, oriented, placed, then moved by the wind.
fn model_position(in: VertexInput, instance: ModelInstance) -> vec3<f32> {
    let local = orient(in.position, instance.position_turns.w, instance.scale_mirror.y)
        * instance.scale_mirror.x;
    let base = instance.position_turns.xyz;
    let bent = bend_plant(local, instance.scale_mirror.zw);
    return wind(bent, base, plant_phase(base), instance.tint.a);
}

// A plant bent by something pushing through it (the player): the bend is simulated on the CPU
// for each plant (a damped spring, see game/src/trample.rs) and given per instance. The whole
// plant bends from its foot: each point moves sideways by `bend` × its height, then is brought
// back to its distance from the foot, so the plant arcs rather than shears.
fn bend_plant(local: vec3<f32>, bend: vec2<f32>) -> vec3<f32> {
    if (local.y <= 0.0 || (bend.x == 0.0 && bend.y == 0.0)) {
        return local;
    }
    let moved = local + vec3<f32>(bend.x, 0.0, bend.y) * local.y;
    return normalize(moved) * length(local);
}

@vertex
fn vs_model(in: VertexInput, instance: ModelInstance) -> VertexOutput {
    var out: VertexOutput;
    let world_position = model_position(in, instance) + wrap_shift(instance.position_turns.xyz);
    out.clip_position = camera.view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    // Mirroring flips the normal's x like the positions'.
    out.normal = orient(in.normal, instance.position_turns.w, instance.scale_mirror.y);
    out.cell = in.cell;
    out.ao = in.ao;
    out.tint = instance.tint.rgb;
    out.cuttable = 1.0;
    return out;
}

@vertex
fn vs_model_shadow(in: VertexInput, instance: ModelInstance) -> @builtin(position) vec4<f32> {
    // Same wind as the main pass: shadows move with the plants.
    let p = model_position(in, instance) + wrap_shift(instance.position_turns.xyz);
    return camera.light_view_proj * vec4<f32>(p, 1.0);
}

// ---------- Articulated parts: a full transform per instance (characters) ----------

// Rust side: `PartInstance` in src/models.rs (per-instance attributes).
struct PartInstance {
    @location(4) column0: vec4<f32>,
    @location(5) column1: vec4<f32>,
    @location(6) column2: vec4<f32>,
    @location(7) column3: vec4<f32>,
    @location(8) tint: vec4<f32>,
}

fn part_transform(instance: PartInstance) -> mat4x4<f32> {
    return mat4x4<f32>(instance.column0, instance.column1, instance.column2, instance.column3);
}

@vertex
fn vs_part(in: VertexInput, instance: PartInstance) -> VertexOutput {
    var out: VertexOutput;
    let transform = part_transform(instance);
    let world_position = (transform * vec4<f32>(in.position, 1.0)).xyz
        + wrap_shift(instance.column3.xyz);
    out.clip_position = camera.view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    // Rotation and uniform scale only: the matrix turns normals too (renormalised later).
    out.normal = (transform * vec4<f32>(in.normal, 0.0)).xyz;
    out.cell = in.cell;
    out.ao = in.ao;
    out.tint = instance.tint.rgb;
    out.cuttable = 0.0;
    return out;
}

@vertex
fn vs_part_shadow(in: VertexInput, instance: PartInstance) -> @builtin(position) vec4<f32> {
    let world_position = (part_transform(instance) * vec4<f32>(in.position, 1.0)).xyz
        + wrap_shift(instance.column3.xyz);
    return camera.light_view_proj * vec4<f32>(world_position, 1.0);
}
