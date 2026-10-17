//! GUI-14
//! A studio: fonts, a GPU, an offscreen target, an animator, and what it takes to draw the screen of
//! a headless sim into it. The screenshots, the goldens and the timings are all made here, through
//! the same animator, scene builder and renderer the window uses.
use crate::anim::Animator;
use crate::api::{Resolver, Shape};
use crate::font::{FontBook, FontConfig};
use crate::frame::Snapshot;
use crate::gpu::{self, Stats};
use crate::headless::Headless;
use crate::metrics::Metrics;
use crate::offscreen::{Offscreen, write_png};
use crate::overlay::{cursor_layer, preedit_layer};
use crate::scene::{Plane, Scene};
use std::path::Path;

/// The margin around the grid, in dp: room for the shadows of the boxes.
pub const MARGIN_DP: f32 = 22.0;

pub struct Studio {
    pub fonts: FontBook,
    pub off: Offscreen,
    pub anim: Animator,
    resolver: Resolver,
}

/// The metrics a font gives, with the window's margin.
pub fn metrics_of(fonts: &FontBook) -> Metrics {
    let mut m = fonts.metrics;
    m.pad_x = (MARGIN_DP * m.scale).round();
    m.pad_y = m.pad_x;
    m
}

impl Studio {
    /// `reduced`: no motion, every frame is a settled one (the screenshots'); else the animator runs
    /// and `tick` moves its time.
    pub fn new(
        cfg: &FontConfig,
        scale: f32,
        size: (u32, u32),
        adapter: Option<&str>,
        reduced: bool,
    ) -> Result<Studio, String> {
        let fonts = FontBook::new(cfg, scale, 2048);
        let gpu = gpu::headless(adapter)?;
        let off = Offscreen::new(gpu, size, 2048);
        Ok(Studio { fonts, off, anim: Animator::new(reduced), resolver: Resolver::new() })
    }

    pub fn metrics(&self) -> Metrics {
        metrics_of(&self.fonts)
    }

    /// Sets the size of the target and tells sim the grid that fits it.
    pub fn fit(&mut self, h: &mut Headless, size: (u32, u32)) {
        self.off.resize(size);
        let (cols, rows) = self.metrics().grid_for(size.0, size.1);
        h.resize(cols, rows);
    }

    /// The animator takes sim's current screen as what the picture is going to.
    pub fn feed(&mut self, h: &Headless) {
        let m = self.metrics();
        self.anim.set_target(Snapshot::take(h.screen(), &mut self.resolver), &m);
    }

    /// The scene and the overlay of this moment. `preedit` is text an input method is composing.
    pub fn compose(&self, preedit: Option<&str>) -> Option<(Scene, Plane)> {
        let m = self.metrics();
        let scene = self.anim.scene(m, self.off.size)?;
        let overlay = match (&scene.cursor, self.anim.snapshot()) {
            (Some(c), Some(snap)) => match preedit {
                Some(p) => {
                    preedit_layer(p, (c.rect.x, c.rect.y), &snap.tones, &m, scene.floor[0] > 0.5).0
                }
                None => {
                    let (rect, block) = self.anim.cursor_now(c, &m);
                    cursor_layer(c, rect, block, &m)
                }
            },
            _ => Plane::default(),
        };
        Some((scene, overlay))
    }

    /// Draws this moment; the stats of the frame.
    pub fn render(&mut self, preedit: Option<&str>) -> Stats {
        let Some((scene, overlay)) = self.compose(preedit) else { return Stats::default() };
        self.off.render(&scene, &overlay, &mut self.fonts)
    }

    /// Takes sim's screen and draws it, settled.
    pub fn frame(&mut self, h: &Headless) -> Stats {
        self.feed(h);
        self.anim.settle();
        self.render(None)
    }

    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        let px = self.off.pixels();
        write_png(path, self.off.size, &px)
    }

    /// The cursor's shape, for the tests.
    pub fn cursor_shape(&self) -> Option<Shape> {
        self.compose(None).and_then(|(s, _)| s.cursor.map(|c| c.shape))
    }
}
