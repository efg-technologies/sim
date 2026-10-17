//! GUI-16
//! Text: the fonts, the shaping of a char, its raster in the atlas. cosmic-text finds the font (the
//! system's monospace or the one asked for, then a fallback for the chars it lacks: CJK, symbols,
//! emoji), shapes a char with its combining marks, and swash draws it. The grid is sim's, not the
//! font's: every glyph is placed at the origin of its cell, so a fallback font with another advance
//! cannot shift the columns.
use crate::metrics::Metrics;
use cosmic_text::{
    Attrs, AttrsOwned, Buffer, CacheKey, Family, FontSystem, Shaping, Style, SwashCache,
    SwashContent, Weight, fontdb,
};
use std::collections::HashMap;

static DEJAVU: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");
static DEJAVU_BOLD: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono-Bold.ttf");
static DEJAVU_ITALIC: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono-Oblique.ttf");

/// The platform's own monospace font, by name where `fontdb` does not read it from the system (macOS:
/// Menlo); elsewhere the generic `monospace`, which `fontdb` has from the fontconfig files.
pub const NATIVE_MONOSPACE: Option<&str> =
    if cfg!(target_os = "macos") { Some("Menlo") } else { None };

/// The bundled font, and what the text is set in where the platform has no monospace font at all.
pub const BUNDLED_FAMILY: &str = "DejaVu Sans Mono";

#[derive(Clone, Debug)]
pub struct FontConfig {
    /// Families to try (comma separated in `SIM_GUI_FONT`); empty, or none of them installed: the
    /// platform's own monospace font (`default_family`).
    pub families: Vec<String>,
    /// The size of the text, in device-independent pixels.
    pub size_dp: f32,
    /// Only the fonts bundled with this crate (the tests'): nothing of the system's.
    pub embedded_only: bool,
    pub locale: Option<String>,
}

impl Default for FontConfig {
    fn default() -> Self {
        FontConfig { families: vec![], size_dp: 15.0, embedded_only: false, locale: None }
    }
}

impl FontConfig {
    /// From `SIM_GUI_FONT` (a family, or several separated by commas), `SIM_GUI_FONT_SIZE` (dp) and
    /// `SIM_GUI_LOCALE` (else the locale of the environment: `ja_JP.UTF-8` is `ja-JP`).
    pub fn from_env() -> FontConfig {
        let mut c = FontConfig::default();
        if let Ok(f) = std::env::var("SIM_GUI_FONT") {
            c.families =
                f.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        }
        if let Some(s) = std::env::var("SIM_GUI_FONT_SIZE").ok().and_then(|s| s.parse::<f32>().ok())
        {
            c.size_dp = s.clamp(6.0, 64.0);
        }
        c.locale = std::env::var("SIM_GUI_LOCALE").ok().or_else(|| {
            ["LC_ALL", "LC_MESSAGES", "LANG"]
                .iter()
                .find_map(|k| {
                    std::env::var(k).ok().filter(|v| !v.is_empty() && v != "C" && v != "POSIX")
                })
                .map(|v| v.split('.').next().unwrap_or("").replace('_', "-"))
        });
        c
    }
}

/// One shaped glyph of a cluster, with where its image goes relative to the cell's origin.
#[derive(Clone, Copy)]
struct Placed {
    key: CacheKey,
    x: i32,
    y: i32,
}

#[derive(Clone)]
struct Cluster {
    glyphs: Vec<Placed>,
    /// The font the first glyph is from has no bold face: the glyph is drawn twice, a pixel apart.
    fake_bold: bool,
    /// The advance of the first glyph, in pixels: what a wide cell centers it by.
    advance: f32,
}

/// A glyph's image in the atlas.
#[derive(Clone, Copy, Debug)]
struct Entry {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    left: i32,
    top: i32,
    color: bool,
}

/// Where a glyph is on the screen and in the atlas, for the renderer.
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub uv: [f32; 4],
    pub color: bool,
}

/// A square atlas of RGBA, filled in rows ("shelves"). Full means flushed and filled again.
pub struct Atlas {
    pub size: u32,
    pub pixels: Vec<u8>,
    shelf: (u32, u32, u32), // x, y, height of the current shelf
    map: HashMap<CacheKey, Option<Entry>>,
    /// The rows [y0, y1) the GPU has not seen.
    pub dirty: Option<(u32, u32)>,
    pub flushed: bool,
}

impl Atlas {
    fn new(size: u32) -> Atlas {
        Atlas {
            size,
            pixels: vec![0; (size * size * 4) as usize],
            shelf: (1, 1, 0),
            map: HashMap::new(),
            dirty: None,
            flushed: false,
        }
    }

    fn clear(&mut self) {
        self.pixels.fill(0);
        self.shelf = (1, 1, 0);
        self.map.clear();
        self.dirty = Some((0, self.size));
        self.flushed = true;
    }

    /// Room for a w x h image; None if the atlas is full.
    fn alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        let (mut x, mut y, mut sh) = self.shelf;
        if x + w + 1 > self.size {
            (x, y, sh) = (1, y + sh + 1, 0);
        }
        if y + h + 1 > self.size || w + 2 > self.size {
            return None;
        }
        self.shelf = (x + w + 1, y, sh.max(h));
        Some((x, y))
    }
}

pub struct FontBook {
    fs: FontSystem,
    swash: SwashCache,
    family: Option<String>,
    buffer: Buffer,
    font_px: f32,
    pub metrics: Metrics,
    clusters: HashMap<(String, bool, bool), Option<Cluster>>,
    pub atlas: Atlas,
    /// The name of the family text is set in (what the system gave), for the logs.
    pub family_name: String,
    /// The font has the glyphs of the Nerd Fonts' icons and powerline (U+E0B0, U+F07B).
    pub has_icons: bool,
}

fn has_family(db: &fontdb::Database, name: &str) -> bool {
    db.faces().any(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)))
}

/// The family text is set in when none was asked for (or none asked for is installed): `native` (Menlo
/// on macOS) if the system has it, else `None`, which is the generic monospace of the system (the
/// fontconfig files' `monospace`); the bundled font where even that is not installed. The chars a
/// family lacks (CJK, symbols, emoji) fall back by the locale in any case.
pub fn default_family(db: &fontdb::Database, native: Option<&str>) -> Option<String> {
    if let Some(n) = native.filter(|n| has_family(db, n)) {
        return Some(n.to_string());
    }
    let generic = fontdb::Query { families: &[fontdb::Family::Monospace], ..Default::default() };
    db.query(&generic).is_none().then(|| BUNDLED_FAMILY.to_string())
}

impl FontBook {
    /// The fonts for a device scale. `atlas` is the side of the glyph atlas in pixels.
    pub fn new(cfg: &FontConfig, scale: f32, atlas: u32) -> FontBook {
        let mut db = fontdb::Database::new();
        if !cfg.embedded_only {
            db.load_system_fonts();
        }
        for data in [DEJAVU, DEJAVU_BOLD, DEJAVU_ITALIC] {
            db.load_font_data(data.to_vec());
        }
        let wanted: Vec<&str> = if cfg.embedded_only {
            vec![BUNDLED_FAMILY]
        } else {
            cfg.families.iter().map(String::as_str).collect()
        };
        let asked = wanted.iter().find(|n| has_family(&db, n)).map(|s| (*s).to_string());
        let family = asked.or_else(|| default_family(&db, NATIVE_MONOSPACE));
        let generic = db.family_name(&fontdb::Family::Monospace).to_string();
        let locale = cfg.locale.clone().unwrap_or_else(|| "en-US".into());
        let mut fs = FontSystem::new_with_locale_and_db(locale, db);
        let font_px = (cfg.size_dp * scale).round().max(6.0);
        let line = (font_px * 1.45).round();
        let mut buffer = Buffer::new(&mut fs, cosmic_text::Metrics::new(font_px, line));
        buffer.set_size(None, None);
        let mut book = FontBook {
            fs,
            swash: SwashCache::new(),
            family: family.clone(),
            buffer,
            font_px,
            metrics: Metrics {
                cell_w: (font_px * 0.6).round(),
                cell_h: line,
                baseline: (line * 0.75).round(),
                font_px,
                pad_x: 0.0,
                pad_y: 0.0,
                scale,
            },
            clusters: HashMap::new(),
            atlas: Atlas::new(atlas),
            family_name: family.unwrap_or(generic),
            has_icons: false,
        };
        book.measure();
        book.has_icons = book.supports('\u{e0b0}') && book.supports('\u{f07b}');
        book
    }

    fn attrs(&self, bold: bool, italic: bool) -> AttrsOwned {
        let fam = match &self.family {
            Some(n) => Family::Name(n),
            None => Family::Monospace,
        };
        AttrsOwned::new(
            &Attrs::new()
                .family(fam)
                .weight(if bold { Weight::BOLD } else { Weight::NORMAL })
                .style(if italic { Style::Italic } else { Style::Normal }),
        )
    }

    /// Shapes `text`; the glyphs it makes, each with the position of its image. `None` if no font has
    /// anything for it (a glyph id 0 only).
    fn shape(&mut self, text: &str, bold: bool, italic: bool) -> Option<Cluster> {
        let attrs = self.attrs(bold, italic);
        let mut b = self.buffer.borrow_with(&mut self.fs);
        b.set_monospace_width(Some(self.metrics.cell_w));
        b.set_text(text, &attrs.as_attrs(), Shaping::Advanced, None);
        b.shape_until_scroll(false);
        let mut glyphs = vec![];
        let mut fake_bold = false;
        let mut first_font = None;
        let mut advance = 0.0;
        for run in b.layout_runs() {
            for g in run.glyphs {
                if g.glyph_id == 0 {
                    continue;
                }
                let p = g.physical((0.0, run.line_y), 1.0);
                if first_font.is_none() {
                    advance = g.w;
                }
                first_font.get_or_insert(g.font_id);
                glyphs.push(Placed { key: p.cache_key, x: p.x, y: p.y });
            }
        }
        if let (true, Some(id)) = (bold, first_font) {
            fake_bold = self.fs.db().face(id).is_some_and(|f| f.weight.0 < 600);
        }
        (!glyphs.is_empty()).then_some(Cluster { glyphs, fake_bold, advance })
    }

    /// The size of a cell from the font: the advance of an `M`, and where the baseline is in a line.
    fn measure(&mut self) {
        let attrs = self.attrs(false, false);
        let mut b = self.buffer.borrow_with(&mut self.fs);
        b.set_monospace_width(None);
        b.set_text("M", &attrs.as_attrs(), Shaping::Advanced, None);
        b.shape_until_scroll(false);
        for run in b.layout_runs() {
            self.metrics.baseline = run.line_y.round();
            if let Some(g) = run.glyphs.first() {
                self.metrics.cell_w = g.w.round().max(1.0);
            }
        }
    }

    /// Whether the font has a glyph for `c` (the primary or a fallback; not a missing-glyph box).
    pub fn supports(&mut self, c: char) -> bool {
        self.cluster(&c.to_string(), false, false).is_some()
    }

    fn cluster(&mut self, text: &str, bold: bool, italic: bool) -> Option<Cluster> {
        let key = (text.to_string(), bold, italic);
        if let Some(c) = self.clusters.get(&key) {
            return c.clone();
        }
        let c = self.shape(text, bold, italic);
        self.clusters.insert(key, c.clone());
        c
    }

    /// The atlas entry of a glyph image, rasterized on first use. `None` if it has no image (a space)
    /// or the atlas is full (then `atlas.flushed` is set after the flush, and the frame is built again).
    fn entry(&mut self, key: CacheKey) -> Option<Entry> {
        if let Some(e) = self.atlas.map.get(&key) {
            return *e;
        }
        let img = self.swash.get_image_uncached(&mut self.fs, key);
        let e = img.and_then(|img| {
            let (w, h) = (img.placement.width, img.placement.height);
            if w == 0 || h == 0 {
                return None;
            }
            let (x, y) = match self.atlas.alloc(w, h) {
                Some(p) => p,
                None => {
                    self.atlas.clear();
                    self.atlas.alloc(w, h)?
                }
            };
            let size = self.atlas.size;
            let color = img.content == SwashContent::Color;
            for row in 0..h {
                for col in 0..w {
                    let i = ((y + row) * size + x + col) as usize * 4;
                    let s = (row * w + col) as usize;
                    let px = match img.content {
                        SwashContent::Color => [
                            img.data[s * 4],
                            img.data[s * 4 + 1],
                            img.data[s * 4 + 2],
                            img.data[s * 4 + 3],
                        ],
                        _ => [255, 255, 255, img.data.get(s).copied().unwrap_or(0)],
                    };
                    self.atlas.pixels[i..i + 4].copy_from_slice(&px);
                }
            }
            let d = self.atlas.dirty.get_or_insert((y, y + h));
            *d = (d.0.min(y), d.1.max(y + h));
            Some(Entry { x, y, w, h, left: img.placement.left, top: img.placement.top, color })
        });
        self.atlas.map.insert(key, e);
        e
    }

    /// The quads of a glyph at a cell (`x`, `y` its top-left): one for each glyph of its cluster.
    /// A cell of two columns centers a glyph narrower than it.
    #[allow(clippy::too_many_arguments)]
    pub fn quads(
        &mut self,
        ch: char,
        extra: &[char],
        cols: u8,
        x: f32,
        y: f32,
        bold: bool,
        italic: bool,
        out: &mut Vec<Quad>,
    ) {
        let mut text = String::with_capacity(8);
        text.push(ch);
        text.extend(extra);
        let Some(c) = self.cluster(&text, bold, italic) else { return };
        let size = self.atlas.size as f32;
        let shift = if cols >= 2 {
            self.wide_shift(&c, f32::from(cols) * self.metrics.cell_w)
        } else {
            0.0
        };
        for p in &c.glyphs {
            let Some(e) = self.entry(p.key) else { continue };
            let qx = x + shift + (p.x + e.left) as f32;
            let qy = y + (p.y - e.top) as f32;
            let uv = [
                e.x as f32 / size,
                e.y as f32 / size,
                (e.x + e.w) as f32 / size,
                (e.y + e.h) as f32 / size,
            ];
            let q = Quad { x: qx, y: qy, w: e.w as f32, h: e.h as f32, uv, color: e.color };
            out.push(q);
            if c.fake_bold {
                out.push(Quad { x: qx + (self.font_px / 14.0).max(1.0).round(), ..q });
            }
        }
    }

    /// How far to move a glyph right so that its advance is centered in `width` pixels, if it is
    /// narrower (a fallback font's wide glyph in a font whose cells are of another size).
    fn wide_shift(&mut self, c: &Cluster, width: f32) -> f32 {
        ((width - c.advance) / 2.0).max(0.0).floor()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> FontBook {
        FontBook::new(&FontConfig { embedded_only: true, ..FontConfig::default() }, 1.0, 512)
    }

    #[test]
    fn gui_16_the_bundled_font_gives_a_cell() {
        let b = book();
        assert_eq!(b.family_name, "DejaVu Sans Mono");
        let m = b.metrics;
        // DejaVu Sans Mono is 0.602 em wide: 15 px of text is a 9 px cell
        assert_eq!(m.cell_w, 9.0);
        assert!(m.cell_h > m.font_px && m.baseline > 0.0 && m.baseline < m.cell_h, "{m:?}");
    }

    /// A font database that has only the bundled font, whose generic monospace is `generic`.
    fn bundled_db(generic: &str) -> fontdb::Database {
        let mut db = fontdb::Database::new();
        for data in [DEJAVU, DEJAVU_BOLD, DEJAVU_ITALIC] {
            db.load_font_data(data.to_vec());
        }
        db.set_monospace_family(generic);
        db
    }

    #[test]
    fn gui_16_with_no_family_asked_for_the_text_is_in_the_platforms_monospace_and_never_a_name_of_ours()
     {
        // the generic monospace of the system is installed: the text is set in it (no name at all)
        let db = bundled_db("DejaVu Sans Mono");
        assert_eq!(default_family(&db, None), None);
        // macOS has a name of its own, used if the system has it, and the generic one if it has not
        assert_eq!(
            default_family(&db, Some("DejaVu Sans Mono")).as_deref(),
            Some("DejaVu Sans Mono")
        );
        assert_eq!(default_family(&db, Some("Menlo")), None);
        // a system whose monospace is not installed gets the bundled font, not a missing glyph box
        let db = bundled_db("A Font Nobody Has");
        assert_eq!(default_family(&db, None).as_deref(), Some(BUNDLED_FAMILY));
        assert_eq!(NATIVE_MONOSPACE.is_some(), cfg!(target_os = "macos"));
        // nobody's favorite font is in the code
        let src = include_str!("font.rs");
        let owner = ["UDEV", " Gothic"].concat();
        assert_eq!(src.matches(owner.as_str()).count(), 0, "a default family is the platform's");
    }

    #[test]
    fn gui_16_a_family_asked_for_by_name_wins_when_it_is_installed_and_is_ignored_when_it_is_not() {
        let ask = |names: &[&str]| {
            let cfg = FontConfig {
                families: names.iter().map(ToString::to_string).collect(),
                ..FontConfig::default()
            };
            FontBook::new(&cfg, 1.0, 512).family_name
        };
        // (the tests' system may have any fonts: DejaVu is the one both have)
        assert_eq!(ask(&["No Such Family Anywhere", "DejaVu Sans Mono"]), "DejaVu Sans Mono");
        let none = ask(&["No Such Family Anywhere"]);
        assert!(!none.is_empty() && none != "No Such Family Anywhere", "the platform's: {none}");
    }

    #[test]
    fn gui_16_a_char_has_a_glyph_and_a_space_has_none() {
        let mut b = book();
        let mut q = vec![];
        b.quads('a', &[], 1, 0.0, 0.0, false, false, &mut q);
        assert_eq!(q.len(), 1);
        assert!(q[0].w > 0.0 && q[0].h > 0.0);
        q.clear();
        b.quads(' ', &[], 1, 0.0, 0.0, false, false, &mut q);
        assert!(q.is_empty());
    }

    #[test]
    fn gui_16_the_same_glyph_is_in_the_atlas_once() {
        let mut b = book();
        let mut q = vec![];
        b.quads('e', &[], 1, 0.0, 0.0, false, false, &mut q);
        b.quads('e', &[], 1, 90.0, 0.0, false, false, &mut q);
        assert_eq!(q[0].uv, q[1].uv);
        assert_eq!(b.atlas.map.len(), 1);
    }

    #[test]
    fn gui_16_bold_is_a_face_of_its_own() {
        let mut b = book();
        let (mut r, mut d) = (vec![], vec![]);
        b.quads('H', &[], 1, 0.0, 0.0, false, false, &mut r);
        b.quads('H', &[], 1, 0.0, 0.0, true, false, &mut d);
        assert_ne!(r[0].uv, d[0].uv);
        assert_eq!(d.len(), 1, "the bundled bold needs no faking");
    }

    #[test]
    fn gui_16_a_full_atlas_is_flushed_and_filled_again() {
        let mut b =
            FontBook::new(&FontConfig { embedded_only: true, ..FontConfig::default() }, 1.0, 64);
        let mut q = vec![];
        for c in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".chars() {
            b.quads(c, &[], 1, 0.0, 0.0, false, false, &mut q);
        }
        assert!(b.atlas.flushed, "52 glyphs do not fit in 64 x 64");
    }
}
