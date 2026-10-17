// Two small pipelines, no loops, no data-dependent branching beyond a few ifs on per-instance
// constants, no compute: a rounded surface (a signed-distance box with a soft shadow, a rim and a
// sheen) and a textured glyph quad. Colors are sRGB-encoded numbers blended as they are, the way a
// terminal blends them, so a color sim derives is the color on the screen.

struct Globals {
    size: vec2<f32>,
    pad: vec2<f32>,
}

@group(0) @binding(0) var<uniform> g: Globals;

fn to_ndc(p: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(p.x / g.size.x * 2.0 - 1.0, 1.0 - p.y / g.size.y * 2.0, 0.0, 1.0);
}

fn corner(vi: u32) -> vec2<f32> {
    return vec2<f32>(f32(vi & 1u), f32((vi >> 1u) & 1u));
}
