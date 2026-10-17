// ---- surfaces ----

struct Surf {
    rect: vec4<f32>,    // x, y, w, h in pixels
    fill: vec4<f32>,    // straight RGBA
    border: vec4<f32>,  // straight RGBA, drawn inside the edge
    shadow: vec4<f32>,  // straight RGBA
    p: vec4<f32>,       // radius, border width, shadow blur, shadow dy
    sheen: vec4<f32>,   // toward white at the top, toward black at the bottom
}

@group(1) @binding(0) var<storage, read> surfs: array<Surf>;

struct SurfOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) idx: u32,
}

@vertex
fn vs_surface(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> SurfOut {
    let s = surfs[ii];
    let half = s.rect.zw * 0.5;
    let center = s.rect.xy + half;
    // room for the shadow around the rectangle
    let ext = s.p.z * 1.2 + abs(s.p.w) + 2.0;
    let c = corner(vi) * 2.0 - vec2<f32>(1.0, 1.0);
    let local = c * (half + vec2<f32>(ext, ext));
    var o: SurfOut;
    o.pos = to_ndc(center + local);
    o.local = local;
    o.idx = ii;
    return o;
}

fn sd_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs_surface(in: SurfOut) -> @location(0) vec4<f32> {
    let s = surfs[in.idx];
    let half = s.rect.zw * 0.5;
    let r = min(s.p.x, min(half.x, half.y));
    let d = sd_box(in.local, half, r);
    let cover = 1.0 - smoothstep(-0.5, 0.5, d);

    // the body: fill, a sheen from the top, a rim just inside the edge
    let ty = clamp(in.local.y / max(half.y, 1.0) * 0.5 + 0.5, 0.0, 1.0);
    var rgb = s.fill.rgb;
    rgb = rgb + (vec3<f32>(1.0, 1.0, 1.0) - rgb) * s.sheen.x * (1.0 - ty) - rgb * s.sheen.y * ty;
    let bw = s.p.y;
    if (bw > 0.0) {
        let ring = smoothstep(-bw - 0.5, -bw + 0.5, d);
        rgb = mix(rgb, s.border.rgb, s.border.a * ring * mix(1.6, 0.5, ty));
    }
    let body_a = s.fill.a * cover;

    // the shadow: the same shape, moved down, with a soft edge
    var sh = 0.0;
    if (s.p.z > 0.0 && s.shadow.a > 0.0) {
        let ds = sd_box(in.local - vec2<f32>(0.0, s.p.w), half, r);
        let t = clamp(0.5 - ds / (2.0 * s.p.z), 0.0, 1.0);
        sh = s.shadow.a * t * t * (3.0 - 2.0 * t);
    }

    // premultiplied: the body over the shadow
    let a = body_a + sh * (1.0 - body_a);
    let c = rgb * body_a + s.shadow.rgb * sh * (1.0 - body_a);
    return vec4<f32>(c, a);
}
