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
    colors: array<vec4<f32>, 16>,  // offset 0, size 256 (rgb + unused w)
}

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> atmosphere: Atmosphere;
@group(0) @binding(2) var<uniform> materials: Materials;

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
    return out;
}

// ---------- Shared helpers ----------

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

// Sun (Lambert: light ∝ cos of the angle to the sun) plus hemispheric ambient (faces looking
// up see the cool sky, faces looking down the warm ground), the ambient dimmed by occlusion.
fn shade(albedo: vec3<f32>, n: vec3<f32>, ao: f32) -> vec3<f32> {
    let sun = atmosphere.sun_color.rgb * max(dot(n, atmosphere.sun_direction.xyz), 0.0);
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
    let cell = unpack_cell(in.cell);
    let base_value = textureLoad(base_field, cell, 0).r;
    var base: vec3<f32>;
    if (volume.base_range.z > 0.5) {
        // Material id in the integer part, a small brightness variation in the fraction.
        let id = min(u32(base_value), 15u);
        base = materials.colors[id].rgb * (0.88 + 0.24 * fract(base_value));
    } else {
        base = base_palette(unit(base_value, volume.base_range.x, volume.base_range.y));
    }
    let life_value = textureLoad(life_field, cell, 0).r;
    let living = life_palette(unit(life_value, volume.life_range.z, volume.life_range.w));
    let coverage = smoothstep(volume.life_range.x, volume.life_range.y, life_value);
    let albedo = mix(base, living, coverage);

    let color = shade(albedo, normalize(in.normal), in.ao);
    return vec4<f32>(haze(color, in.world_position), 1.0);
}

// ---------- Transparent water ----------

// Opacity of the shallowest and of the deepest water.
const WATER_ALPHA_MIN: f32 = 0.6;
const WATER_ALPHA_MAX: f32 = 0.85;
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
    if (n.y > 0.5) {
        let tilt = ripple(in.world_position.xz, camera.time.x);
        n = normalize(vec3<f32>(tilt.x, 1.0, tilt.y));
    }
    var color = shade(albedo, n, 1.0);

    // Glint: the sun mirrored by the surface, seen when the reflection points at the eye.
    let to_eye = normalize(camera.eye.xyz - in.world_position);
    let mirrored = reflect(-atmosphere.sun_direction.xyz, n);
    let glint = pow(max(dot(mirrored, to_eye), 0.0), GLINT_SHARPNESS) * GLINT_STRENGTH;
    color += atmosphere.sun_color.rgb * glint * (1.0 - foam);

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
    let color = shade(in.color, normalize(in.normal), 1.0);
    return vec4<f32>(haze(color, in.world_position), 1.0);
}
