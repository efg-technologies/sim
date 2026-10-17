//! GUI-14
//! Rendering with no window: a texture of the size asked, drawn into by the same `Renderer` the window
//! uses, read back, and written as a PNG. What the screenshots, the goldens and the timings are made of.
use crate::font::FontBook;
use crate::gpu::{Gpu, Renderer, Stats};
use crate::scene::{Plane, Scene};
use std::path::Path;

pub struct Offscreen {
    pub gpu: Gpu,
    pub renderer: Renderer,
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    readback: wgpu::Buffer,
    pub size: (u32, u32),
    padded_row: u32,
}

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

impl Offscreen {
    pub fn new(gpu: Gpu, size: (u32, u32), atlas: u32) -> Offscreen {
        let renderer = Renderer::new(&gpu, FORMAT, atlas);
        let (tex, view, readback, padded_row) = Offscreen::target(&gpu.device, size);
        Offscreen { gpu, renderer, tex, view, readback, size, padded_row }
    }

    fn target(
        device: &wgpu::Device,
        (w, h): (u32, u32),
    ) -> (wgpu::Texture, wgpu::TextureView, wgpu::Buffer, u32) {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        let padded_row = (w * 4).div_ceil(256) * 256;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded_row) * u64::from(h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        (tex, view, readback, padded_row)
    }

    pub fn resize(&mut self, size: (u32, u32)) {
        if size != self.size {
            (self.tex, self.view, self.readback, self.padded_row) =
                Offscreen::target(&self.gpu.device, size);
            self.size = size;
        }
    }

    /// Draws a frame and waits for the GPU to finish it. The stats have the time the GPU took.
    pub fn render(&mut self, scene: &Scene, overlay: &Plane, fonts: &mut FontBook) -> Stats {
        let mut stats = self.renderer.render(&self.view, self.size, scene, overlay, fonts);
        stats.gpu = self.renderer.wait();
        stats
    }

    /// The last frame as RGBA, rows tightly packed.
    pub fn pixels(&self) -> Vec<u8> {
        let (w, h) = self.size;
        let mut enc = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("readback") });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.gpu.queue.submit([enc.finish()]);
        let slice = self.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| drop(tx.send(r)));
        let _ = self.gpu.device.poll(wgpu::PollType::wait_indefinitely());
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        if rx.recv().is_ok_and(|r| r.is_ok()) {
            if let Ok(data) = slice.get_mapped_range() {
                for row in 0..h as usize {
                    let start = row * self.padded_row as usize;
                    out.extend_from_slice(&data[start..start + (w * 4) as usize]);
                }
            }
            self.readback.unmap();
        }
        out
    }
}

/// Writes RGBA pixels (opaque) as an RGB PNG, to a new file renamed over `path` (see `safe_io`: a
/// symlink at `path` is replaced, never written through).
pub fn write_png(path: &Path, size: (u32, u32), rgba: &[u8]) -> Result<(), String> {
    let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    crate::safe_io::write_replace(path, |f| {
        let mut enc = png::Encoder::new(std::io::BufWriter::new(f), size.0, size.1);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(std::io::Error::other)?;
        w.write_image_data(&rgb).map_err(std::io::Error::other)
    })
    .map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads a PNG written by `write_png` back as RGBA.
pub fn read_png(path: &Path) -> Result<((u32, u32), Vec<u8>), String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut reader =
        png::Decoder::new(std::io::BufReader::new(file)).read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; reader.output_buffer_size().ok_or("png too large")?];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let data = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgb => data.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::Rgba => data.to_vec(),
        other => return Err(format!("unexpected png color type {other:?}")),
    };
    Ok(((info.width, info.height), rgba))
}
