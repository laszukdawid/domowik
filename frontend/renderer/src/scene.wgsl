// Domowik 3D scene shader (issue 0101). Same lighting as the legacy Linux GL
// renderer so the two paths can be compared during migration.
//
// Clip space is WebGPU's: depth in [0, 1], framebuffer row 0 at the top, which
// is also Flutter's texture row order, so no Y reflection is applied here.

struct Camera {
    clip_from_world: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var base_color: texture_2d_array<f32>;
@group(1) @binding(1) var base_sampler: sampler;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    @location(3) m0: vec4<f32>,
    @location(4) m1: vec4<f32>,
    @location(5) m2: vec4<f32>,
    @location(6) m3: vec4<f32>,
    @location(7) n0: vec3<f32>,
    @location(8) n1: vec3<f32>,
    @location(9) n2: vec3<f32>,
    @location(10) tint: vec4<f32>,
    @location(11) uv_layer: vec3<f32>,
    @location(12) wrap: vec2<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) layer: i32,
    @location(4) @interpolate(flat) wrap: vec2<f32>,
};

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    let model = mat4x4<f32>(v.m0, v.m1, v.m2, v.m3);
    var out: VertexOut;
    out.clip = camera.clip_from_world * model * vec4<f32>(v.position, 1.0);
    out.normal = mat3x3<f32>(v.n0, v.n1, v.n2) * v.normal;
    out.color = v.color * v.tint;
    out.uv = v.uv_layer.xy;
    out.layer = i32(v.uv_layer.z);
    out.wrap = v.wrap;
    return out;
}

@fragment
fn fs_main(in: VertexOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var uv = in.uv;
    let half_texel = 0.5 / vec2<f32>(textureDimensions(base_color));
    for (var i = 0u; i < 2u; i++) {
        if (in.wrap[i] == 1.0) { uv[i] = clamp(uv[i], half_texel[i], 1.0 - half_texel[i]); }
        if (in.wrap[i] == 2.0) { uv[i] = clamp(1.0 - abs((uv[i] - 2.0 * floor(uv[i] / 2.0)) - 1.0), half_texel[i], 1.0 - half_texel[i]); }
    }
    let texel = textureSample(base_color, base_sampler, uv, max(in.layer, 0));
    var color = in.color;
    if (in.layer >= 0) {
        let linear = select(pow((color.rgb + 0.055) / 1.055, vec3<f32>(2.4)), color.rgb / 12.92, color.rgb <= vec3<f32>(0.04045)) * texel.rgb;
        color = vec4<f32>(select(1.055 * pow(linear, vec3<f32>(1.0 / 2.4)) - 0.055, linear * 12.92, linear <= vec3<f32>(0.0031308)), color.a * texel.a);
    }
    if (color.a < 0.01) {
        discard;
    }
    var n = normalize(in.normal);
    if (!front) {
        n = -n;
    }
    let light = 0.38 + 0.62 * max(dot(n, normalize(vec3<f32>(0.35, 0.85, 0.4))), 0.0);
    return vec4<f32>(color.rgb * light, color.a);
}
