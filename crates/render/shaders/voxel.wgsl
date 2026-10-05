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
    time: vec4<f32>,         // offset 80: x = seconds since start, yzw unused
    light_view_proj: mat4x4<f32>,  // offset 96, size 64: world → sun's shadow map
}

// Rust side: `AtmosphereUniform` in src/palette.rs. `w` unused unless stated.
struct Atmosphere {
    sun_direction: vec4<f32>,  // offset 0, towards the sun, normalised
    sun_color: vec4<f32>,      // offset 16
    sky_color: vec4<f32>,      // offset 32
    ground_color: vec4<f32>,   // offset 48
    fog_color: vec4<f32>,      // offset 64
    fog: vec4<f32>,            // offset 80: start, end, max, unused
}

// Rust side: `MaterialsUniform` in src/palette.rs. Indexed by material id.
struct Materials {
    colors: array<vec4<f32>, 32>,  // offset 0, size 512 (rgb + unused w)
}

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> atmosphere: Atmosphere;
@group(0) @binding(2) var<uniform> materials: Materials;
// Depth of the first lit surface seen from the sun, per texel (see `vs_shadow`).
@group(0) @binding(3) var shadow_map: texture_depth_2d;
// Compares a depth with the map instead of returning it: 1 = lit, 0 = in shadow, and with
// linear filtering a blend of the 4 nearest comparisons.
@group(0) @binding(4) var shadow_sampler: sampler_comparison;

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
}

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Mesh positions are in grid cells; the volume's origin places them in the world.
    let world_position = in.position + volume.origin.xyz;
    out.clip_position = camera.view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    out.normal = in.normal;
    out.cell = in.cell;
    out.ao = in.ao;
    out.tint = vec3<f32>(1.0);
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

// Wind on plants, after "Vegetation Procedural Animation and Shading in Crysis" (GPU Gems 3,
// ch. 16): a main bending of the whole plant along the wind, and a detail bending of its crown.
//
// Wind, blowing towards +x and a bit +z (same as the particles in the air, game/ambient.rs).
const WIND_DIRECTION: vec3<f32> = vec3<f32>(0.89, 0.0, 0.45);
// Horizontal direction across the wind, for side-to-side motion.
const ACROSS_WIND: vec3<f32> = vec3<f32>(-0.45, 0.0, 0.89);
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
    let along = dot(base, WIND_DIRECTION) * 0.012;
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
    var moved = local + WIND_DIRECTION * (MAIN_BEND / 0.254 * flexibility * lean * bend);
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
    moved += ACROSS_WIND * (DETAIL_SIDE * crown * side);
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
    if (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || ndc.z > 1.0) {
        return 1.0;
    }
    var lit = 0.0;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let offset = vec2<f32>(f32(dx), f32(dy)) * SHADOW_TEXEL;
            lit += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + offset, ndc.z);
        }
    }
    return lit / 9.0;
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
    return mix(color, atmosphere.fog_color.rgb, amount);
}

// ---------- Opaque volumes ----------

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Albedo: the inert colour, covered by the overlay colour as the overlay value grows.
    let direct = (in.cell & DIRECT_MATERIAL) != 0u;
    let value = base_value(in.cell);
    var base: vec3<f32>;
    if (direct || volume.base_range.z > 0.5) {
        // Material id in the integer part, a small brightness variation in the fraction.
        let id = min(u32(value), 31u);
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
    let color = shade(albedo, n, in.ao, sun_visibility(in.world_position, n));
    return vec4<f32>(haze(color, in.world_position), 1.0);
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
    if (n.y > 0.5) {
        let tilt = ripple(in.world_position.xz, camera.time.x);
        n = normalize(vec3<f32>(tilt.x, 1.0, tilt.y));
    }
    var color = shade(albedo, n, 1.0, sunlit);

    // Glint: the sun mirrored by the surface, seen when the reflection points at the eye.
    let to_eye = normalize(camera.eye.xyz - in.world_position);
    let mirrored = reflect(-atmosphere.sun_direction.xyz, n);
    let glint = pow(max(dot(mirrored, to_eye), 0.0), GLINT_SHARPNESS) * GLINT_STRENGTH;
    color += atmosphere.sun_color.rgb * glint * (1.0 - foam) * sunlit;

    // Shallow water lets the ground show through; deep water and foam hide it.
    let alpha = clamp(mix(WATER_ALPHA_MIN, WATER_ALPHA_MAX, depth_t) + 0.4 * foam, 0.0, 1.0);
    return vec4<f32>(haze(color, in.world_position), alpha);
}

// ---------- Particles: one cube mesh drawn once per instance ----------

// Rust side: `ParticleInstance` in src/particles.rs (per instance) and the cube vertices.
struct ParticleInput {
    @location(0) corner: vec3<f32>,   // cube vertex, in [-0.5, 0.5]³
    @location(1) normal: vec3<f32>,
    @location(2) centre_size: vec4<f32>,  // instance: centre xyz, edge length w
    @location(3) color: vec4<f32>,        // instance: linear rgb, a unused
}

struct ParticleOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
}

@vertex
fn vs_particle(in: ParticleInput) -> ParticleOutput {
    var out: ParticleOutput;
    let world_position = in.centre_size.xyz + in.corner * in.centre_size.w;
    out.clip_position = camera.view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    out.normal = in.normal;
    out.color = in.color.rgb;
    return out;
}

@fragment
fn fs_particle(in: ParticleOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let color = shade(in.color, n, 1.0, sun_visibility(in.world_position, n));
    return vec4<f32>(haze(color, in.world_position), 1.0);
}

// ---------- Shadow pass: depth of the scene seen from the sun ----------

// Only the position matters: the depth buffer of this pass is the shadow map.
@vertex
fn vs_shadow(in: VertexInput) -> @builtin(position) vec4<f32> {
    return camera.light_view_proj * vec4<f32>(in.position + volume.origin.xyz, 1.0);
}

// ---------- Models: one mesh drawn at many places (plants) ----------

// Rust side: `ModelInstance` in src/models.rs (per-instance attributes).
struct ModelInstance {
    @location(4) position_turns: vec4<f32>,  // where the model stands (xyz), quarter turns (w)
    @location(5) scale_mirror: vec4<f32>,    // size (x), mirrored before turning (y: 1 or 0)
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
    return wind(local, base, plant_phase(base), instance.tint.a);
}

@vertex
fn vs_model(in: VertexInput, instance: ModelInstance) -> VertexOutput {
    var out: VertexOutput;
    let world_position = model_position(in, instance);
    out.clip_position = camera.view_proj * vec4<f32>(world_position, 1.0);
    out.world_position = world_position;
    // Mirroring flips the normal's x like the positions'.
    out.normal = orient(in.normal, instance.position_turns.w, instance.scale_mirror.y);
    out.cell = in.cell;
    out.ao = in.ao;
    out.tint = instance.tint.rgb;
    return out;
}

@vertex
fn vs_model_shadow(in: VertexInput, instance: ModelInstance) -> @builtin(position) vec4<f32> {
    // Same wind as the main pass: shadows move with the plants.
    return camera.light_view_proj * vec4<f32>(model_position(in, instance), 1.0);
}
