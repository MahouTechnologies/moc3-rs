struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

// @group(0) @binding(0)
// var<uniform> u_mvp: mat4x4<f32>;

@group(0) @binding(0)
var texture : texture_2d<f32>;
@group(0) @binding(1)
var texture_sampler : sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(texture, texture_sampler, in.uv);
}