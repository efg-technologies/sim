//! GUI-9, GUI-14
//! The GPU side: an adapter and a device, and a renderer that draws a `Scene` with two pipelines
//! (rounded surfaces and glyph quads) into any texture view: an offscreen texture for the goldens
//! and the screenshots, a window's surface for the app.
//!
//! The work is small and bounded on purpose (the GPU here also drives the owner's desktop, and a
//! hang resets it): one render pass per frame, a few hundred surfaces and a few thousand glyph quads,
//! shaders without loops, no compute, and every frame waited for before the next is started.
use crate::font::{FontBook, Quad};
use crate::scene::{Glyph, Kind, Plane, Rect, Scene, Surface};
use bytemuck::{Pod, Zeroable};
use std::time::{Duration, Instant};

pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    pub fn info(&self) -> wgpu::AdapterInfo {
        self.adapter.get_info()
    }

    /// A one-line name of the adapter: its name, the backend and the driver.
    pub fn describe(&self) -> String {
        let i = self.info();
        format!(
            "{} ({:?}, {:?}, driver {} {})",
            i.name, i.device_type, i.backend, i.driver, i.driver_info
        )
    }
}

pub fn instance() -> wgpu::Instance {
    wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN | wgpu::Backends::METAL,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    })
}

/// The names of the adapters the instance can see.
pub fn adapters(instance: &wgpu::Instance) -> Vec<wgpu::AdapterInfo> {
    pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()))
        .iter()
        .map(wgpu::Adapter::get_info)
        .collect()
}

/// The adapter whose name contains `hint` (case-insensitive), else the high-performance one. With
/// `compatible` the adapter can present to that surface.
pub fn pick(
    instance: &wgpu::Instance,
    hint: Option<&str>,
    compatible: Option<&wgpu::Surface<'_>>,
) -> Result<wgpu::Adapter, String> {
    if let Some(h) = hint.filter(|h| !h.is_empty()) {
        let h = h.to_lowercase();
        let all = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
        if let Some(a) = all.into_iter().find(|a| a.get_info().name.to_lowercase().contains(&h)) {
            return Ok(a);
        }
        return Err(format!(
            "no adapter named like {h:?}; there are: {}",
            adapters(instance).iter().map(|i| i.name.clone()).collect::<Vec<_>>().join(", ")
        ));
    }
    pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: compatible,
        apply_limit_buckets: false,
    }))
    .map_err(|e| format!("no adapter: {e}"))
}

pub fn device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), String> {
    pollster::block_on(
        adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("sim-gui"),
            ..Default::default()
        }),
    )
    .map_err(|e| format!("no device: {e}"))
}

/// A GPU with no window, for offscreen renders. The adapter is named by `hint` or
/// `SIM_GUI_ADAPTER` (a part of its name, e.g. `llvmpipe` or `9070`), else the high-performance one.
pub fn headless(hint: Option<&str>) -> Result<Gpu, String> {
    let env = std::env::var("SIM_GUI_ADAPTER").ok();
    let instance = instance();
    let adapter = pick(&instance, hint.or(env.as_deref()), None)?;
    let (device, queue) = device(&adapter)?;
    Ok(Gpu { instance, adapter, device, queue })
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SurfInst {
    rect: [f32; 4],
    fill: [f32; 4],
    border: [f32; 4],
    shadow: [f32; 4],
    p: [f32; 4],
    sheen: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct QuadInst {
    rect: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
    flags: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    size: [f32; 2],
    pad: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pipe {
    Surf,
    Quad,
}

/// One draw call: instances `start..end` of a kind, with a scissor.
#[derive(Clone, Copy, Debug)]
struct Draw {
    kind: Pipe,
    start: u32,
    end: u32,
    scissor: Option<[u32; 4]>,
}

/// What a frame cost and held.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub surfaces: usize,
    pub glyphs: usize,
    pub draws: usize,
    /// Making the instances (the scene to the GPU's data, with the glyphs shaped and rasterized).
    pub pack: Duration,
    /// Encoding, submitting, and waiting until the GPU is done (offscreen only; zero for a window).
    pub gpu: Duration,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
    surf_pipe: wgpu::RenderPipeline,
    glyph_pipe: wgpu::RenderPipeline,
    storage_bgl: wgpu::BindGroupLayout,
    globals: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    atlas_tex: wgpu::Texture,
    atlas_bg: wgpu::BindGroup,
    surf_buf: Buffer,
    quad_buf: Buffer,
    atlas_size: u32,
}

/// A storage buffer that grows, with the bind group of its current allocation.
struct Buffer {
    buf: wgpu::Buffer,
    bg: wgpu::BindGroup,
    cap: u64,
    stride: u64,
}

impl Buffer {
    fn new(device: &wgpu::Device, bgl: &wgpu::BindGroupLayout, stride: u64, items: u64) -> Buffer {
        let cap = items.max(64);
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: cap * stride,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("instances"),
            layout: bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
        });
        Buffer { buf, bg, cap, stride }
    }

    fn fit(&mut self, device: &wgpu::Device, bgl: &wgpu::BindGroupLayout, items: u64) {
        if items > self.cap {
            *self = Buffer::new(device, bgl, self.stride, (items * 3 / 2).max(self.cap * 2));
        }
    }
}

const COMMON: &str = include_str!("shaders/common.wgsl");
const SURFACE: &str = include_str!("shaders/surface.wgsl");
const GLYPH: &str = include_str!("shaders/glyph.wgsl");

impl Renderer {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat, atlas_size: u32) -> Renderer {
        let (device, queue) = (gpu.device.clone(), gpu.queue.clone());
        let module = |label: &str, body: &str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(format!("{COMMON}\n{body}").into()),
            })
        };
        let (surf_mod, glyph_mod) = (module("surface", SURFACE), module("glyph", GLYPH));
        let visible = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let globals_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: visible,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let storage_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("instances"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: visible,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let atlas_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atlas"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = |label: &str, groups: &[&wgpu::BindGroupLayout]| {
            let groups: Vec<Option<&wgpu::BindGroupLayout>> =
                groups.iter().map(|g| Some(*g)).collect();
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: &groups,
                ..Default::default()
            })
        };
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let pipe = |label: &str,
                    module: &wgpu::ShaderModule,
                    vs: &str,
                    fs: &str,
                    layout: &wgpu::PipelineLayout| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let surf_pipe = pipe(
            "surface",
            &surf_mod,
            "vs_surface",
            "fs_surface",
            &layout("surface", &[&globals_bgl, &storage_bgl]),
        );
        let glyph_pipe = pipe(
            "glyph",
            &glyph_mod,
            "vs_glyph",
            "fs_glyph",
            &layout("glyph", &[&globals_bgl, &storage_bgl, &atlas_bgl]),
        );

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        let atlas_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("atlas"),
            size: wgpu::Extent3d {
                width: atlas_size,
                height: atlas_size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let atlas_view = atlas_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("atlas"),
            layout: &atlas_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let surf_buf =
            Buffer::new(&device, &storage_bgl, std::mem::size_of::<SurfInst>() as u64, 512);
        let quad_buf =
            Buffer::new(&device, &storage_bgl, std::mem::size_of::<QuadInst>() as u64, 4096);
        Renderer {
            device,
            queue,
            format,
            surf_pipe,
            glyph_pipe,
            storage_bgl,
            globals,
            globals_bg,
            atlas_tex,
            atlas_bg,
            surf_buf,
            quad_buf,
            atlas_size,
        }
    }

    /// Draws the scene (and an overlay layer over all of it: the cursor, the input method's text)
    /// into `target`, which is `size` pixels. Returns when the commands are submitted; offscreen
    /// callers wait with `wait`.
    pub fn render(
        &mut self,
        target: &wgpu::TextureView,
        size: (u32, u32),
        scene: &Scene,
        overlay: &Plane,
        fonts: &mut FontBook,
    ) -> Stats {
        let t0 = Instant::now();
        let (mut surfs, mut quads, mut draws);
        let mut tries = 0;
        loop {
            fonts.atlas.flushed = false;
            (surfs, quads, draws) = (vec![], vec![], vec![]);
            let mut pack =
                Pack { fonts, surfs: &mut surfs, quads: &mut quads, draws: &mut draws, size };
            for (i, l) in scene.layers.iter().enumerate() {
                if let Some((_, c)) = scene.scrim.filter(|(at, _)| *at == i) {
                    pack.scrim(c);
                }
                pack.layer(l);
            }
            pack.layer(overlay);
            tries += 1;
            // a flush in the middle of a frame leaves the quads before it pointing at gone images
            if !fonts.atlas.flushed || tries > 2 {
                break;
            }
        }
        let pack_time = t0.elapsed();
        let stats = Stats {
            surfaces: surfs.len(),
            glyphs: quads.len(),
            draws: draws.len(),
            pack: pack_time,
            gpu: Duration::ZERO,
        };

        // upload what changed
        if let Some((y0, y1)) = fonts.atlas.dirty.take() {
            let row = self.atlas_size as usize * 4;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: y0, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &fonts.atlas.pixels[y0 as usize * row..y1 as usize * row],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row as u32),
                    rows_per_image: Some(y1 - y0),
                },
                wgpu::Extent3d {
                    width: self.atlas_size,
                    height: y1 - y0,
                    depth_or_array_layers: 1,
                },
            );
        }
        self.surf_buf.fit(&self.device, &self.storage_bgl, surfs.len() as u64);
        self.quad_buf.fit(&self.device, &self.storage_bgl, quads.len() as u64);
        if !surfs.is_empty() {
            self.queue.write_buffer(&self.surf_buf.buf, 0, bytemuck::cast_slice(&surfs));
        }
        if !quads.is_empty() {
            self.queue.write_buffer(&self.quad_buf.buf, 0, bytemuck::cast_slice(&quads));
        }
        self.queue.write_buffer(
            &self.globals,
            0,
            bytemuck::bytes_of(&Globals { size: [size.0 as f32, size.1 as f32], pad: [0.0; 2] }),
        );

        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        {
            let [r, g, b, _] = scene.floor.map(f64::from);
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            let mut now: Option<Pipe> = None;
            for d in &draws {
                if now != Some(d.kind) {
                    match d.kind {
                        Pipe::Surf => {
                            pass.set_pipeline(&self.surf_pipe);
                            pass.set_bind_group(1, &self.surf_buf.bg, &[]);
                        }
                        Pipe::Quad => {
                            pass.set_pipeline(&self.glyph_pipe);
                            pass.set_bind_group(1, &self.quad_buf.bg, &[]);
                            pass.set_bind_group(2, &self.atlas_bg, &[]);
                        }
                    }
                    now = Some(d.kind);
                }
                let [x, y, w, h] = d.scissor.unwrap_or([0, 0, size.0, size.1]);
                pass.set_scissor_rect(x, y, w.max(1), h.max(1));
                pass.draw(0..4, d.start..d.end);
            }
        }
        self.queue.submit([enc.finish()]);
        stats
    }

    /// Waits until everything submitted is done; how long it took.
    pub fn wait(&self) -> Duration {
        let t = Instant::now();
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        t.elapsed()
    }
}

/// The scene's lists made into instances and draw calls.
struct Pack<'a> {
    fonts: &'a mut FontBook,
    surfs: &'a mut Vec<SurfInst>,
    quads: &'a mut Vec<QuadInst>,
    draws: &'a mut Vec<Draw>,
    size: (u32, u32),
}

impl Pack<'_> {
    fn scissor(&self, clip: Option<Rect>) -> Option<[u32; 4]> {
        let c = clip?;
        let (x0, y0) = (c.x.floor().max(0.0) as u32, c.y.floor().max(0.0) as u32);
        let (x1, y1) = (
            (c.right().ceil() as u32).min(self.size.0),
            (c.bottom().ceil() as u32).min(self.size.1),
        );
        Some([
            x0.min(self.size.0),
            y0.min(self.size.1),
            x1.saturating_sub(x0),
            y1.saturating_sub(y0),
        ])
    }

    fn draw(&mut self, kind: Pipe, scissor: Option<[u32; 4]>) {
        let end = match kind {
            Pipe::Surf => self.surfs.len(),
            Pipe::Quad => self.quads.len(),
        } as u32;
        match self.draws.last_mut() {
            Some(d) if d.kind == kind && d.scissor == scissor && d.end + 1 == end => d.end = end,
            _ => self.draws.push(Draw { kind, start: end - 1, end, scissor }),
        }
    }

    fn surface(&mut self, s: &Surface) {
        let (sh, blur, dy) = match s.shadow {
            Some(sh) => (sh.color, sh.blur, sh.dy),
            None => ([0.0; 4], 0.0, 0.0),
        };
        let (border, bw) = s.border.unwrap_or(([0.0; 4], 0.0));
        self.surfs.push(SurfInst {
            rect: [s.rect.x, s.rect.y, s.rect.w, s.rect.h],
            fill: s.fill,
            border,
            shadow: sh,
            p: [s.radius, bw, blur, dy],
            sheen: [s.sheen.0, s.sheen.1, 0.0, 0.0],
        });
        let sc = self.scissor(s.clip);
        self.draw(Pipe::Surf, sc);
    }

    fn scrim(&mut self, c: [f32; 4]) {
        let rect = Rect::new(0.0, 0.0, self.size.0 as f32, self.size.1 as f32);
        self.surface(&Surface {
            rect,
            radius: 0.0,
            fill: c,
            border: None,
            shadow: None,
            sheen: (0.0, 0.0),
            kind: Kind::Plain,
            clip: None,
        });
    }

    fn glyph(&mut self, g: &Glyph) {
        let mut qs = vec![];
        self.fonts.quads(g.ch, &g.extra, g.cols, g.x, g.y, g.bold, g.italic, &mut qs);
        let sc = self.scissor(g.clip);
        for q in qs {
            self.push_quad(q, g.color, sc);
        }
    }

    fn push_quad(&mut self, q: Quad, color: [f32; 4], sc: Option<[u32; 4]>) {
        self.quads.push(QuadInst {
            rect: [q.x, q.y, q.w, q.h],
            uv: q.uv,
            color,
            flags: [if q.color { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
        });
        self.draw(Pipe::Quad, sc);
    }

    fn layer(&mut self, l: &Plane) {
        for s in &l.surfaces {
            self.surface(s);
        }
        for g in &l.glyphs {
            self.glyph(g);
        }
    }
}
