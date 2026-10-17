//! GUI-9, GUI-12, GUI-14, MD-9
//! Goldens for the renderer, in two layers.
//!
//! The scene goldens (`tests/golden/scene_*.txt`) are the display list as text: every surface with its
//! rectangle, radius, color, shadow and rim, every run of text with its color. They need no GPU and no
//! font, so they are exact and a change in the layers or the elevations shows up in review as a diff.
//!
//! The pixel goldens (`tests/golden/pixels_*.png`) are the same scenes through the GPU with the bundled
//! font, on the software adapter (llvmpipe) when there is one, compared with a tolerance since GPUs
//! round differently. They are skipped (and say so) when no adapter can be had. A failure writes the
//! picture it got and the difference next to the target directory.
//!
//! `SIM_GUI_BLESS=1 cargo test --test golden` rewrites them.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::chunks_exact_to_as_chunks
)] // helpers; the tests themselves may unwrap
use sim_gui::api::{Chg, Resolver, Rgb, apca};
use sim_gui::font::FontConfig;
use sim_gui::frame::Snapshot;
use sim_gui::headless::Headless;
use sim_gui::metrics::TEST;
use sim_gui::offscreen::{read_png, write_png};
use sim_gui::scene::{Kind, Scene, build};
use sim_gui::studio::Studio;
use sim_gui::theme::{DARK, LIGHT, Theme};
use std::path::{Path, PathBuf};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn bless() -> bool {
    std::env::var_os("SIM_GUI_BLESS").is_some()
}

fn check_text(name: &str, actual: &str) {
    let path = golden_dir().join(name);
    if bless() {
        std::fs::create_dir_all(golden_dir()).unwrap();
        sim_gui::safe_io::write_replace(&path, |f| std::io::Write::write_all(f, actual.as_bytes()))
            .unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        actual == want,
        "{name} differs from {} (SIM_GUI_BLESS=1 rewrites it)\n--- got ---\n{}",
        path.display(),
        actual.lines().take(40).collect::<Vec<_>>().join("\n")
    );
}

// ---- the scenes ----

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    file: &'static str,
    theme: Theme,
    drive: fn(&mut Headless),
}

fn tree_markdown(h: &mut Headless) {
    h.keys("<C-n>");
}

fn picker(h: &mut Headless) {
    h.keys("<C-n><C-p>rs");
}

fn hidden(h: &mut Headless) {
    h.session.sim.trust = Some(false);
    h.keys("j");
}

/// which-key: the leader typed (an inline shell's pause is over at once).
fn which_key(h: &mut Headless) {
    h.keys("<C-n><Esc> ");
}

/// The changes view: what differs from HEAD, with a rename, a deletion and a name cut with a `…`.
fn changes(h: &mut Headless) {
    sim_gui::demo::diverge(&h.dir).unwrap();
    h.keys(" ge");
}

/// HEAD's version of the deleted file, from the changes view.
fn head_tab(h: &mut Headless) {
    changes(h);
    h.walk_to("build.sh").keys("\n");
}

/// A cursor at the top of a long file: the rows far from it are faded.
fn fog_drive(h: &mut Headless) {
    h.keys("<C-n><Esc>");
}

/// MD-9: the cursor on a row of the table: that row is the file's own text, the rows around it are
/// read (aligned, their markers gone).
fn table_row(h: &mut Headless) {
    h.keys("<C-n><Esc>17G");
}

const CASES: [Case; 7] = [
    Case { name: "tree_markdown_dark", file: "README.md", theme: DARK, drive: tree_markdown },
    Case { name: "tree_markdown_light", file: "README.md", theme: LIGHT, drive: tree_markdown },
    Case { name: "picker_dark", file: "README.md", theme: DARK, drive: picker },
    Case { name: "hidden_char_dark", file: "hidden.txt", theme: DARK, drive: hidden },
    Case { name: "which_key_dark", file: "README.md", theme: DARK, drive: which_key },
    Case { name: "changes_dark", file: "README.md", theme: DARK, drive: changes },
    Case { name: "table_row_dark", file: "README.md", theme: DARK, drive: table_row },
];

fn headless(c: &Case, size: (u16, u16)) -> Headless {
    let mut h = Headless::demo(c.name, &[c.file], size, &c.theme);
    (c.drive)(&mut h);
    h
}

fn scene_of(h: &Headless, size: (u16, u16)) -> Scene {
    let snap = Snapshot::take(h.screen(), &mut Resolver::new());
    let px = (
        (f32::from(size.0) * TEST.cell_w + 2.0 * TEST.pad_x) as u32,
        (f32::from(size.1) * TEST.cell_h + 2.0 * TEST.pad_y) as u32,
    );
    build(&snap.view(TEST, px))
}

#[test]
fn gui_14_the_display_lists_are_what_they_were() {
    for c in CASES {
        let size = (100, 30);
        let h = headless(&c, size);
        check_text(&format!("scene_{}.txt", c.name), &scene_of(&h, size).dump());
    }
}

// ---- what the scenes say about depth ----

fn count(s: &Scene, role: Kind) -> usize {
    s.layers.iter().flat_map(|l| &l.surfaces).filter(|x| x.kind == role).count()
}

#[test]
fn gui_9_the_tree_and_the_text_are_two_raised_boxes_with_shadows_and_no_box_drawing_chars() {
    let h = headless(&CASES[0], (100, 30));
    let s = scene_of(&h, (100, 30));
    let boxes: Vec<_> = s.layers[0].surfaces.iter().filter(|x| x.kind == Kind::Box).collect();
    assert_eq!(boxes.len(), 2);
    for b in &boxes {
        assert!(
            b.shadow.is_some_and(|sh| sh.blur > 0.0 && sh.dy > 0.0 && sh.color[3] > 0.3),
            "soft, below, visible"
        );
        assert!(b.radius > 4.0, "rounded");
        assert!(b.border.is_some(), "a rim");
    }
    let glyphs: String = s.layers[0].glyphs.iter().map(|g| g.ch).collect();
    for c in "╭╮╰╯│─├┤┬┴┼".chars() {
        assert!(!glyphs.contains(c), "{c} is drawn by a font: the lines are shapes");
    }
    assert!(count(&s, Kind::Heading) >= 2, "heading bands are surfaces");
    assert_eq!(count(&s, Kind::Band), 1, "the fenced block is one surface");
    assert!(s.scrim.is_none() && s.layers.len() == 1);
    // the surfaces under text are the page's (sim's palette is made for it), not a lifted copy
    assert!(boxes.iter().all(|b| b.fill == s.page));
}

#[test]
fn gui_9_the_picker_floats_above_everything_on_a_scrim_with_the_largest_shadow() {
    let h = headless(&CASES[2], (100, 30));
    let s = scene_of(&h, (100, 30));
    assert_eq!(s.layers.len(), 2, "the content and the picker");
    let (over, scrim) = s.scrim.expect("the content behind is dimmed");
    assert_eq!(over, 1, "the scrim lies over the page and under the picker");
    assert!(scrim[3] >= 0.3);
    let float = s.layers[1].surfaces.iter().find(|x| x.kind == Kind::Float).expect("the window");
    let boxes: Vec<_> = s.layers[0].surfaces.iter().filter(|x| x.kind == Kind::Box).collect();
    let (bs, fs) = (boxes[0].shadow.unwrap(), float.shadow.unwrap());
    assert!(fs.blur > 2.0 * bs.blur && fs.dy > 2.0 * bs.dy, "higher is softer and further");
    assert!(float.radius > boxes[0].radius);
    // nothing of the picker is in the layer under the scrim, and the picker's text is above it
    let under: String = s.layers[0].glyphs.iter().map(|g| g.ch).collect();
    assert!(!under.contains("Findfiles"));
    let over: String = s.layers[1].glyphs.iter().map(|g| g.ch).collect();
    assert!(over.contains("Findfiles"), "(blanks are no glyphs)");
}

#[test]
fn gui_9_a_hidden_char_is_a_rounded_pill_in_the_alert_color_and_its_literal_twin_is_text() {
    let h = headless(&CASES[3], (100, 30));
    let snap = Snapshot::take(h.screen(), &mut Resolver::new());
    let s = scene_of(&h, (100, 30));
    let (alert, _) = snap.tones.alert_block();
    let pills: Vec<_> = s.layers[0].surfaces.iter().filter(|x| x.kind == Kind::Pill).collect();
    // the real U+202E and the real escape char on the third line, and the real U+202E on the first, and the
    // untrusted root's mark in the status line
    assert!(pills.len() >= 3, "{} pills", pills.len());
    for p in &pills {
        assert_eq!(p.radius, p.rect.h / 2.0, "a pill is round at both ends");
        assert_eq!(
            [
                (p.fill[0] * 255.0).round() as u8,
                (p.fill[1] * 255.0).round() as u8,
                (p.fill[2] * 255.0).round() as u8
            ],
            [alert.0, alert.1, alert.2]
        );
        assert!(p.shadow.is_some_and(|g| g.color[3] > 0.4), "it glows in its own color");
    }
    // the literal text `<U+202E>` of the second line has no pill under it
    let y2 = TEST.pad_y + (f32::from(h.session.sim.geo().y0) + 1.0) * TEST.cell_h;
    assert!(
        pills.iter().all(|p| (p.rect.y - y2).abs() > 2.0 || p.rect.x > 400.0),
        "the second line has none"
    );
}

#[test]
fn gui_9_which_key_floats_on_no_scrim_and_a_band_is_one_level_below_a_box() {
    // a window that is not modal is lit by its shadow alone: the keys still go on, so nothing dims
    let h = headless(&CASES[4], (100, 30));
    let s = scene_of(&h, (100, 30));
    assert_eq!(s.layers.len(), 2, "the content and which-key");
    assert!(s.scrim.is_none(), "which-key does not dim what is behind it");
    assert_eq!(count(&s, Kind::Float), 1);
    // the levels: a heading's band, a fence's band and a chip sit one level above the page (1 / 4), the
    // boxes four (4 / 16), a window sixteen (16 / 40)
    let h = headless(&CASES[0], (100, 30));
    let s = scene_of(&h, (100, 30));
    let shadow =
        |k: Kind| s.layers[0].surfaces.iter().find(|x| x.kind == k).and_then(|x| x.shadow).unwrap();
    let (band, boxed) = (shadow(Kind::Heading), shadow(Kind::Box));
    assert_eq!((band.dy, band.blur), (TEST.dpf(1.0), TEST.dpf(4.0)));
    assert_eq!((boxed.dy, boxed.blur), (TEST.dpf(4.0), TEST.dpf(16.0)));
    assert_eq!(shadow(Kind::Band).blur, band.blur);
    assert_eq!(shadow(Kind::Chip).blur, band.blur, "a chip is on the page, as a band is");
}

#[test]
fn gui_9_the_changes_view_is_the_tree_s_box_with_numbers_in_text_colors_and_a_name_cut_with_dots() {
    let h = headless(&CASES[5], (100, 30));
    let snap = Snapshot::take(h.screen(), &mut Resolver::new());
    let s = scene_of(&h, (100, 30));
    // the same surfaces as the tree: two boxes, the cursor's row, and no surface of the view's own
    let kinds: Vec<Kind> = s.layers[0].surfaces.iter().map(|x| x.kind).collect();
    assert_eq!(kinds.iter().filter(|k| **k == Kind::Box).count(), 2);
    assert_eq!(kinds.iter().filter(|k| **k == Kind::Select).count(), 1, "the cursor row");
    assert_eq!(s.layers.len(), 1);
    // the numbers are glyphs in the add and del text colors; the letters are glyphs too
    let text: String = s.layers[0].glyphs.iter().map(|g| g.ch).collect();
    for what in ["\u{2026}", "\u{2192}", "+", "\u{2212}"] {
        assert!(text.contains(what), "the view draws {what:?}");
    }
    let colors = |ch: char| -> Vec<Rgb> {
        s.layers[0]
            .glyphs
            .iter()
            .filter(|g| g.ch == ch && g.x < 400.0)
            .map(|g| rgb8(g.color))
            .collect()
    };
    let (add, del) = (snap.tones.change_fg(Chg::Add), snap.tones.change_fg(Chg::Del));
    assert!(colors('+').contains(&add), "{:?} against {add:?}", colors('+'));
    assert!(colors('\u{2212}').contains(&del), "{:?} against {del:?}", colors('\u{2212}'));
}

#[test]
fn gui_9_the_head_tab_is_a_chip_like_any_tab_and_the_text_has_the_focus() {
    let c = Case { name: "head", file: "README.md", theme: DARK, drive: head_tab };
    let h = headless(&c, (100, 30));
    let s = scene_of(&h, (100, 30));
    let chips = s.layers[0].surfaces.iter().filter(|x| x.kind == Kind::Chip).count();
    assert!(chips >= 1, "the shown tab: {chips}");
    let text: String = s.layers[0].glyphs.iter().map(|g| g.ch).collect();
    assert!(text.contains("HEAD:scripts/build.sh") || text.contains("HEAD:build.sh"), "{text}");
    assert!(
        s.layers[0].surfaces.iter().all(|x| x.kind != Kind::Select),
        "no cursor row in the tree"
    );
}

/// The glyphs of the page layer by the cell they are drawn at.
fn glyph_at(s: &Scene, x: f32, y: f32) -> Option<[f32; 4]> {
    s.layers[0]
        .glyphs
        .iter()
        .find(|g| (g.x - x).abs() < 0.5 && (g.y - y).abs() < 0.5)
        .map(|g| g.color)
}

fn rgb8(c: [f32; 4]) -> Rgb {
    let b = |v: f32| (v * 255.0).round() as u8;
    (b(c[0]), b(c[1]), b(c[2]))
}

#[test]
fn gui_12_the_fog_drawn_never_goes_under_the_floors_and_the_secondary_gray_does_not_fade() {
    for theme in [DARK, LIGHT] {
        let c = Case { name: "fog", file: "src/dense.rs", theme, drive: fog_drive };
        let h = headless(&c, (100, 30));
        let snap = Snapshot::take(h.screen(), &mut Resolver::new());
        let page = snap.grid.pal.bg;
        // the farthest the fog goes: every cell that fades is 100% faded
        let fog: Vec<Vec<u8>> = snap
            .grid
            .rows
            .iter()
            .map(|r| r.cells.iter().map(|c| if c.r.fade > 0 { 100 } else { 0 }).collect())
            .collect();
        let px = (
            (100.0 * TEST.cell_w + 2.0 * TEST.pad_x) as u32,
            (30.0 * TEST.cell_h + 2.0 * TEST.pad_y) as u32,
        );
        let mut view = snap.view(TEST, px);
        view.fog = Some(&fog);
        let scene = build(&view);
        let (mut faded, mut gray) = (0, 0);
        for (r, row) in snap.grid.rows.iter().enumerate() {
            for c in &row.cells {
                if c.ch == ' ' || sim_gui::blocks::pieces(c.ch).is_some() {
                    continue;
                }
                let x = TEST.pad_x + f32::from(c.col) * TEST.cell_w;
                let y = TEST.pad_y + r as f32 * TEST.cell_h;
                let Some(drawn) = glyph_at(&scene, x, y).map(rgb8) else { continue };
                let lc = apca(drawn, page).abs();
                if c.r.fade > 0 {
                    // the floor: 75 for text, 40 for the numbers (a step of the bisection less)
                    let floor = if c.r.soft { 40.0 } else { 75.0 };
                    let own = c.r.fg.map_or(floor, |f| apca(f, page).abs());
                    assert!(
                        lc >= floor.min(own) - 1.5,
                        "{theme:?} row {r} {:?}: |Lc| {lc:.1}",
                        c.ch
                    );
                    assert_eq!(Some(drawn), c.r.ink(page, 100), "it is sim's own fade_to, at 100");
                    faded += 1;
                } else if c.r.fg == Some(snap.grid.pal.secondary) {
                    assert_eq!(
                        drawn, snap.grid.pal.secondary,
                        "the secondary gray is rank, not distance"
                    );
                    gray += 1;
                }
            }
        }
        assert!(faded > 50, "{theme:?}: a screen of text has fog: {faded}");
        assert!(gray > 0 || theme.bg.0 > 200, "{theme:?}: and comments that do not fade");
    }
}

// ---- pixels ----

/// A studio on the software adapter. Only llvmpipe: a test never draws on the machine's own GPU (it
/// may be the one that drives the desktop); with no llvmpipe the pixel tests skip, and say so.
fn studio(size: (u32, u32)) -> Option<Studio> {
    let cfg = FontConfig { embedded_only: true, ..FontConfig::default() };
    match Studio::new(&cfg, 1.0, size, Some("llvmpipe"), true) {
        Ok(s) => {
            eprintln!("adapter: {}", s.off.gpu.describe());
            Some(s)
        }
        Err(e) => {
            eprintln!("no llvmpipe adapter: {e}");
            None
        }
    }
}

/// Compares pixels with a tolerance: GPUs round blends and shadows differently by a few levels.
fn compare(name: &str, size: (u32, u32), got: &[u8]) {
    let path = golden_dir().join(name);
    if bless() {
        std::fs::create_dir_all(golden_dir()).unwrap();
        write_png(&path, size, got).unwrap();
    }
    let (wsize, want) = read_png(&path).unwrap_or(((0, 0), vec![]));
    assert_eq!(wsize, size, "{name}: the golden's size (SIM_GUI_BLESS=1 rewrites it)");
    let (mut over, mut worst) = (0usize, 0u8);
    for (a, b) in got.chunks_exact(4).zip(want.chunks_exact(4)) {
        let d = (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap_or(0);
        worst = worst.max(d);
        over += usize::from(d > 8);
    }
    let total = got.len() / 4;
    if over * 1000 > total {
        let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/golden-diff");
        let _ = std::fs::create_dir_all(&out);
        let _ = write_png(&out.join(name), size, got);
        panic!(
            "{name}: {over} of {total} pixels differ by more than 8 levels (worst {worst}); the picture it drew is in {}",
            out.display()
        );
    }
}

fn pixels(c: &Case, size: (u32, u32)) -> Option<Vec<u8>> {
    let mut st = studio(size)?;
    let mut h = Headless::demo(c.name, &[c.file], (80, 24), &c.theme);
    st.fit(&mut h, size);
    (c.drive)(&mut h);
    st.frame(&h);
    Some(st.off.pixels())
}

#[test]
fn gui_14_pixels_of_the_picker_over_the_tree_and_markdown() {
    let size = (960, 540);
    let Some(px) = pixels(&CASES[2], size) else { return eprintln!("skipped: no GPU adapter") };
    compare("pixels_picker_dark.png", size, &px);
}

#[test]
fn gui_14_pixels_of_the_light_palette() {
    let size = (960, 540);
    let Some(px) = pixels(&CASES[1], size) else { return eprintln!("skipped: no GPU adapter") };
    compare("pixels_tree_markdown_light.png", size, &px);
}

#[test]
fn gui_14_pixels_of_the_changes_view() {
    let size = (960, 540);
    let Some(px) = pixels(&CASES[5], size) else { return eprintln!("skipped: no GPU adapter") };
    compare("pixels_changes_dark.png", size, &px);
}

#[test]
fn md_9_pixels_of_a_table_in_the_read_view() {
    let size = (960, 540);
    let Some(px) = pixels(&CASES[6], size) else { return eprintln!("skipped: no GPU adapter") };
    compare("pixels_table_row_dark.png", size, &px);
}

#[test]
fn md_9_a_table_in_the_read_view_is_the_same_cells_as_the_terminal_draws() {
    let size = (100, 30);
    let h = headless(&CASES[6], size);
    let dump = scene_of(&h, size).dump();
    // the x of the first run `word` on the line of the scene at height `y`
    let at = |y: u32, word: &str| -> f32 {
        let key = format!("{y}] \"{word}\"");
        let line =
            dump.lines().find(|l| l.contains(&key)).unwrap_or_else(|| panic!("{key}\n{dump}"));
        let x = line.split('[').nth(1).and_then(|r| r.split(' ').next());
        x.and_then(|x| x.parse().ok()).unwrap()
    };
    // `what` of the header and `the` of the second row are in one column: the cells are aligned by
    // their width, and the row the cursor is on (17) is the file's own text, one column to the left
    let (header, cursor, second) = (at(356, "what"), at(396, "the"), at(416, "the"));
    assert_eq!(header, second, "the columns of the read view");
    assert!(cursor < second, "the cursor's row is raw: {cursor} against {second}");
    assert!(!dump.contains("\"**sim**\"") && !dump.contains("\"---\""), "read, not raw");
    check_text("scene_table_row_dark.txt", &dump);
}

#[test]
fn gui_9_the_pixels_say_depth_too_a_shadow_a_pill_a_dimmed_backdrop() {
    let size = (960, 540);
    let Some(mut st) = studio(size) else { return eprintln!("skipped: no GPU adapter") };
    let mut h = Headless::demo("px-hidden", &["hidden.txt"], (80, 24), &DARK);
    st.fit(&mut h, size);
    h.session.sim.trust = Some(true);
    h.keys("j");
    st.frame(&h);
    let (scene, _) = st.compose(None).unwrap();
    let px = st.off.pixels();
    let at = |x: f32, y: f32| {
        let i = (y as usize * size.0 as usize + x as usize) * 4;
        [px[i], px[i + 1], px[i + 2]]
    };
    let rgb8 = |c: [f32; 4]| {
        [(c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8]
    };
    // the floor, far from everything, is the floor; the box is the page
    assert_eq!(at(2.0, 2.0), rgb8(scene.floor));
    let b = scene.layers[0].surfaces.iter().find(|s| s.kind == Kind::Box).unwrap();
    assert_eq!(
        at(b.rect.x + b.rect.w - 60.0, b.rect.y + b.rect.h - 60.0),
        rgb8(scene.page),
        "the box's inside is the page"
    );
    // beside the box's edge the shadow darkens the floor
    let below = at(b.rect.x - 6.0, b.rect.y + b.rect.h / 2.0);
    let sum = |c: [u8; 3]| u32::from(c[0]) + u32::from(c[1]) + u32::from(c[2]);
    assert!(
        sum(below) < sum(rgb8(scene.floor)),
        "a shadow under the box: {below:?} against the floor {:?}",
        rgb8(scene.floor)
    );
    // the pill is the alert's color at its center
    let pill = scene.layers[0].surfaces.iter().find(|s| s.kind == Kind::Pill).unwrap();
    let c = at(pill.rect.x + pill.rect.w / 2.0, pill.rect.y + 3.0);
    let want = rgb8(pill.fill);
    assert!((0..3).all(|i| c[i].abs_diff(want[i]) < 40), "the pill's color {c:?} against {want:?}");
}
