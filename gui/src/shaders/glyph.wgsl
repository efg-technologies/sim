// ---- glyphs ----

struct Quad {
    rect: vec4<f32>,    // x, y, w, h
    uv: vec4<f32>,      // u0, v0, u1, v1
    color: vec4<f32>,   // straight RGBA
    flags: vec4<f32>,   // x: 1 for a color glyph
}

@group(1) @binding(0) var<storage, read> quads: array<Quad>;
@group(2) @binding(0) var atlas: texture_2d<f32>;
@group(2) @binding(1) var atlas_sampler: sampler;

struct QuadOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) @interpolate(flat) idx: u32,
}

@vertex
fn vs_glyph(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> QuadOut {
    let q = quads[ii];
    let c = corner(vi);
    var o: QuadOut;
    o.pos = to_ndc(q.rect.xy + c * q.rect.zw);
    o.uv = mix(q.uv.xy, q.uv.zw, c);
    o.idx = ii;
    return o;
}

@fragment
fn fs_glyph(in: QuadOut) -> @location(0) vec4<f32> {
    let q = quads[in.idx];
    let t = textureSample(atlas, atlas_sampler, in.uv);
    if (q.flags.x > 0.5) {
        let a = t.a * q.color.a;
        return vec4<f32>(t.rgb * a, a);
    }
    let a = t.a * q.color.a;
    return vec4<f32>(q.color.rgb * a, a);
}
