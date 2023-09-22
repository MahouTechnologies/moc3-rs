struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct Uniform {
    multiply_color: vec3<f32>,
    screen_color: vec3<f32>,
    opacity: f32,
}

@group(0) @binding(1)
var<uniform> data: Uniform;

@group(1) @binding(0)
var texture : texture_2d<f32>;
@group(1) @binding(1)
var texture_sampler : sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex = textureSample(texture, texture_sampler, in.uv);
    var color = tex.rgb * data.multiply_color;
    color = (tex.rgb + data.screen_color) - (tex.rgb * data.screen_color);
    color *= tex.a;

    return vec4(color, tex.a);
}