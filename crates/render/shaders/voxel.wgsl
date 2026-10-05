// Voxel faces: transforms vertices with the camera, colours each face from its cell (inert base
// colour blended with the living colour), then lights it: warm sun, cool sky light in the
// shadows, ambient occlusion in the creases, haze with distance.

// ---------- Group 0: data shared by the whole frame ----------

// Rust side: `CameraUniform` in src/camera.rs.
struct Camera {
    view_proj: mat4x4<f32>,  // offset 0, size 64
    eye: vec4<f32>,          // offset 64: eye position, w = distance to the target
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

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<uniform> atmosphere: Atmosphere;

// ---------- Group 1: the volume being drawn (terrain, parcel…) ----------

// Rust side: `VolumeUniform` in src/volume.rs.
struct Volume {
    origin: vec4<f32>,                // offset 0: world position of cell (0,0,0), w unused
    base_stops: array<vec4<f32>, 5>,  // offset 16, size 80 (rgb + unused w)
    life_stops: array<vec4<f32>, 5>,  // offset 96, size 80
    base_range: vec4<f32>,            // offset 176: min, max, unused, unused
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

// ---------- Fragment stage ----------

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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Albedo: what the surface is made of. The inert colour, covered by the living colour
    // as life grows: tinted at the edge of a living patch, fully living at its core.
    let cell = unpack_cell(in.cell);
    let base_value = textureLoad(base_field, cell, 0).r;
    let base = base_palette(unit(base_value, volume.base_range.x, volume.base_range.y));
    let life_value = textureLoad(life_field, cell, 0).r;
    let living = life_palette(unit(life_value, volume.life_range.z, volume.life_range.w));
    let coverage = smoothstep(volume.life_range.x, volume.life_range.y, life_value);
    let albedo = mix(base, living, coverage);

    let n = normalize(in.normal);
    // Direct sun: Lambert's law, light received ∝ cos(angle to the sun).
    let sun = atmosphere.sun_color.rgb * max(dot(n, atmosphere.sun_direction.xyz), 0.0);
    // Hemispheric ambient: faces looking up see the (cool) sky, faces looking down see the
    // (warm) ground. This is what colours the shadows instead of greying them.
    let ambient = mix(atmosphere.ground_color.rgb, atmosphere.sky_color.rgb, n.y * 0.5 + 0.5);
    let occlusion = mix(AO_FLOOR, 1.0, in.ao);
    var color = albedo * (sun + ambient) * occlusion;

    // Haze: grows with the distance beyond the camera target, so the far side of the parcel
    // fades into the background whatever the zoom.
    let depth = distance(in.world_position, camera.eye.xyz) - camera.eye.w;
    let fog = atmosphere.fog;
    let haze = clamp((depth - fog.x) / (fog.y - fog.x), 0.0, 1.0) * fog.z;
    color = mix(color, atmosphere.fog_color.rgb, haze);

    return vec4<f32>(color, 1.0);
}
