// The interface: flat coloured triangles already in clip space (see src/ui.rs), blended
// over the final image.

// Rust side: `UiVertex` in src/ui.rs.
struct UiVertex {
    @location(0) position: vec2<f32>,  // clip space
    @location(1) color: vec4<f32>,     // linear rgb, alpha
}

struct UiOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_ui(in: UiVertex) -> UiOutput {
    var out: UiOutput;
    out.position = vec4<f32>(in.position, 0.0, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_ui(in: UiOutput) -> @location(0) vec4<f32> {
    return in.color;
}
