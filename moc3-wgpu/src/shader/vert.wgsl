struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> u_camera: mat4x4<f32>;

@vertex
fn vs_main(
    @location(0) vertex: vec2<f32>,
    @location(1) uv: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.position = mat4x4f(1.5, 0.0, 0.0, 0.0, 0.0, -1.5, 0.0, 0.0, 0.0, 0.0, 1.5, 0.0, 0.0, 0.0, 0.0, 1.0) * vec4f(vertex, 0.0, 1.0);
    out.uv = uv;
    return out;
}
