//! GUI-14, GUI-15
//! Offscreen renders of sim in its window, to PNG (no window is opened), and the frame times.
//!
//! `gui-shots [OUT_DIR] [--adapter NAME] [--embedded] [--only NAME] [--time]`
//!
//! The scenes are made in a neutral demo project under the system's temp directory (see
//! `sim_gui::demo`); nothing of the user's is read. `SIM_GUI_FONT`, `SIM_GUI_FONT_SIZE` and
//! `SIM_GUI_ADAPTER` apply as they do to the window.
use sim_gui::api::{CtEvent, KeyModifiers, MouseEvent, MouseEventKind, Resolver};
use sim_gui::demo;
use sim_gui::font::FontConfig;
use sim_gui::frame::Snapshot;
use sim_gui::headless::Headless;
use sim_gui::studio::Studio;
use sim_gui::theme::{DARK, LIGHT, Theme};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct Args {
    out: PathBuf,
    adapter: Option<String>,
    embedded: bool,
    only: Option<String>,
    time: bool,
}

const USAGE: &str = "usage: gui-shots OUT_DIR [--adapter NAME] [--embedded] [--only NAME] [--time]\n\
    OUT_DIR is required: a directory of your own (made at mode 0700 if it is not there; refused if it\n\
    is not yours or others can write to it). Nothing is written anywhere else.";

/// The command line. The output directory is required, not defaulted to a path in a shared
/// directory that another user could have prepared.
fn parse(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let (mut out, mut adapter, mut embedded, mut only, mut time) = (None, None, false, None, false);
    let mut it = args.into_iter();
    while let Some(x) = it.next() {
        match x.as_str() {
            "--adapter" => adapter = Some(it.next().ok_or("--adapter needs a name")?),
            "--embedded" => embedded = true,
            "--only" => only = Some(it.next().ok_or("--only needs a name")?),
            "--time" => time = true,
            f if f.starts_with("--") => return Err(format!("unknown option {f}")),
            _ if out.is_some() => return Err("more than one output directory".into()),
            _ => out = Some(PathBuf::from(x)),
        }
    }
    let out = out.ok_or("the output directory (OUT_DIR) is required")?;
    Ok(Args { out, adapter, embedded, only, time })
}

/// A scene: how sim is started and driven.
struct Scene {
    name: &'static str,
    file: &'static str,
    theme: Theme,
    drive: fn(&mut Headless),
    /// Text an input method is composing, drawn at the cursor.
    preedit: Option<&'static str>,
}

fn tree_markdown(h: &mut Headless) {
    h.keys("<C-n>");
}

fn picker(h: &mut Headless) {
    h.keys("<C-n><C-p>rs");
}

fn hidden(h: &mut Headless) {
    h.session.sim.trust = Some(false);
    // an outside process writes to the file while it is open: sim shows it as a change on disk
    let path = h.dir.join("hidden.txt");
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    text.push_str("appended by another process\n");
    let _ = std::fs::write(&path, text);
    let id = h.session.sim.bufs[h.session.sim.cur].id;
    h.session.disk_changed(id, "hidden.txt");
    h.keys("j");
}

/// which-key: a prefix typed (the leader), and a pause on it.
fn which_key(h: &mut Headless) {
    h.keys("<C-n><Esc> ");
}

/// `:help`: the key table, as a file.
fn help(h: &mut Headless) {
    h.keys(":help<CR>");
}

/// The changes view (`<Space>ge`): what differs from HEAD, a file's numbers and letter, a rename, a
/// deleted file and a name too long for the panel (cut with a `…`).
fn changes(h: &mut Headless) {
    if let Err(e) = demo::diverge(&h.dir) {
        eprintln!("cannot change the demo project: {e}");
    }
    h.keys(" ge");
}

/// HEAD's version of the deleted file, opened from the changes view: a read-only tab named `HEAD:...`.
fn head_tab(h: &mut Headless) {
    changes(h);
    h.walk_to("build.sh").keys("\n");
}

/// Japanese text in Insert mode, with the input method's text being composed at the cursor.
fn japanese(h: &mut Headless) {
    h.keys("jjjjA");
}

/// A screen full of text.
fn dense(h: &mut Headless) {
    h.keys("<C-n>");
    h.keys("jjjj");
}

fn scenes() -> Vec<Scene> {
    vec![
        Scene {
            name: "gui-tree-markdown",
            file: "README.md",
            theme: DARK,
            drive: tree_markdown,
            preedit: None,
        },
        Scene { name: "gui-picker", file: "README.md", theme: DARK, drive: picker, preedit: None },
        Scene {
            name: "gui-which-key",
            file: "README.md",
            theme: DARK,
            drive: which_key,
            preedit: None,
        },
        Scene { name: "gui-help", file: "README.md", theme: DARK, drive: help, preedit: None },
        Scene {
            name: "gui-changes",
            file: "README.md",
            theme: DARK,
            drive: changes,
            preedit: None,
        },
        Scene {
            name: "gui-head-tab",
            file: "README.md",
            theme: DARK,
            drive: head_tab,
            preedit: None,
        },
        Scene {
            name: "gui-hidden-char",
            file: "hidden.txt",
            theme: DARK,
            drive: hidden,
            preedit: None,
        },
        Scene {
            name: "gui-light",
            file: "README.md",
            theme: LIGHT,
            drive: tree_markdown,
            preedit: None,
        },
        Scene {
            name: "gui-japanese-ime",
            file: "docs/nihongo.md",
            theme: DARK,
            drive: japanese,
            preedit: Some("にほんご"),
        },
        Scene { name: "gui-dense", file: "src/dense.rs", theme: DARK, drive: dense, preedit: None },
    ]
}

/// What one scene costs to draw, in microseconds: the median and the 95th percentile of the frames.
struct Timing {
    scene: Vec<f64>,
    pack: Vec<f64>,
    gpu: Vec<f64>,
    feed: Vec<f64>,
    surfaces: usize,
    glyphs: usize,
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(((v.len() as f64 - 1.0) * p).round() as usize).copied().unwrap_or(0.0)
}

/// Frames of one scene, drawn one after another (each waited for), after a few that warm the atlas.
fn time_scene(st: &mut Studio, h: &Headless, frames: usize) -> Timing {
    let us = |d: Duration| d.as_secs_f64() * 1e6;
    let mut t =
        Timing { scene: vec![], pack: vec![], gpu: vec![], feed: vec![], surfaces: 0, glyphs: 0 };
    for i in 0..frames + 5 {
        let t0 = Instant::now();
        st.feed(h);
        st.anim.settle();
        let feed = us(t0.elapsed());
        let t1 = Instant::now();
        let Some((scene, overlay)) = st.compose(None) else { return t };
        let build = us(t1.elapsed());
        let stats = st.off.render(&scene, &overlay, &mut st.fonts);
        if i >= 5 {
            t.feed.push(feed);
            t.scene.push(build);
            t.pack.push(us(stats.pack));
            t.gpu.push(us(stats.gpu));
            (t.surfaces, t.glyphs) = (stats.surfaces, stats.glyphs);
        }
    }
    t
}

/// The table of frame times for every scene at the sizes asked for.
fn timings(a: &Args, cfg: &FontConfig) -> String {
    let mut out = String::new();
    let modes: [((u32, u32), f32, &str); 3] = [
        ((1920, 1080), 1.0, "1920x1080 (scale 1)"),
        ((3840, 2160), 2.0, "3840x2160 (scale 2, HiDPI)"),
        ((3840, 2160), 1.0, "3840x2160 (scale 1: four times the cells)"),
    ];
    for (size, scale, label) in modes {
        let Ok(mut st) = Studio::new(cfg, scale, size, a.adapter.as_deref(), true) else {
            continue;
        };
        let _ = writeln!(out, "\n### {label}  -  {}", st.off.gpu.describe());
        let _ = writeln!(
            out,
            "font {} ({}x{} px cells)\n",
            st.fonts.family_name, st.fonts.metrics.cell_w, st.fonts.metrics.cell_h
        );
        let _ = writeln!(
            out,
            "| scene | cells | surfaces | glyph quads | new sim screen (us) | scene (us) | pack (us) | GPU (us) | frame, steady (us) |"
        );
        let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|");
        for s in scenes() {
            let mut h =
                Headless::demo_with(s.name, &[s.file], (100, 30), &s.theme, st.fonts.has_icons);
            st.fit(&mut h, size);
            (s.drive)(&mut h);
            let (cols, rows) = (h.screen().frame.width, h.screen().frame.height);
            let mut t = time_scene(&mut st, &h, 60);
            let (feed, scene, pack, gpu) = (
                pct(&mut t.feed, 0.5),
                pct(&mut t.scene, 0.5),
                pct(&mut t.pack, 0.5),
                pct(&mut t.gpu, 0.5),
            );
            let (p95s, p95p, p95g) =
                (pct(&mut t.scene, 0.95), pct(&mut t.pack, 0.95), pct(&mut t.gpu, 0.95));
            let _ = writeln!(
                out,
                "| {} | {cols}x{rows} | {} | {} | {feed:.0} | {scene:.0} | {pack:.0} | {gpu:.0} | {:.0} (p95 {:.0}) |",
                s.name.trim_start_matches("gui-"),
                t.surfaces,
                t.glyphs,
                scene + pack + gpu,
                p95s + p95p + p95g
            );
        }
    }
    out
}

/// A frame in the middle of a scroll: the text is between two places, clipped to its box, and the
/// cursor (a bar, in Insert mode) is still on its way.
fn scroll_mid(a: &Args, st: &mut Studio) {
    let name = "gui-scroll-mid";
    if a.only.as_deref().is_some_and(|o| !name.contains(o)) {
        return;
    }
    let size = (1920, 1080);
    let mut h = Headless::demo_with(name, &["src/dense.rs"], (100, 30), &DARK, st.fonts.has_icons);
    st.fit(&mut h, size);
    st.anim.reduced = false;
    st.feed(&h);
    st.anim.settle();
    // two notches of the wheel: six lines, as sim scrolls them
    let snap = Snapshot::take(h.screen(), &mut Resolver::new());
    let text = snap.text().map(|(l, ..)| l.rect).unwrap_or_default();
    for _ in 0..2 {
        let wheel = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: text.x + 10,
            row: text.y + 4,
            modifiers: KeyModifiers::NONE,
        };
        h.event(CtEvent::Mouse(wheel));
    }
    st.feed(&h);
    let moving = st.anim.tick(0.045, &st.metrics());
    let scrolling = st.anim.scrolling();
    let stats = st.render(None);
    st.anim.reduced = true;
    let path = a.out.join(format!("{name}.png"));
    match st.save(&path) {
        Ok(()) => println!(
            "{}: still moving: {moving}, scrolling by {scrolling:?} rows, {} glyphs",
            path.display(),
            stats.glyphs
        ),
        Err(e) => eprintln!("{}: {e}", path.display()),
    }
}

fn main() {
    let a = match parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("gui-shots: {e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    if let Err(e) = sim_gui::safe_io::ensure_out_dir(&a.out) {
        eprintln!("gui-shots: {e}");
        std::process::exit(2);
    }
    let mut cfg = FontConfig::from_env();
    cfg.embedded_only = a.embedded;
    if a.time {
        let table = timings(&a, &cfg);
        println!("{table}");
        let file = a.out.join("gui-frame-times.md");
        if let Err(e) = sim_gui::safe_io::write_replace(&file, |f| {
            std::io::Write::write_all(f, table.as_bytes())
        }) {
            eprintln!("{}: {e}", file.display());
        }
        return;
    }
    let size = (1920, 1080);
    let mut st = match Studio::new(&cfg, 1.0, size, a.adapter.as_deref(), true) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("gui-shots: {e}");
            std::process::exit(1);
        }
    };
    println!("gpu: {}", st.off.gpu.describe());
    println!(
        "font: {} (cell {}x{} px)",
        st.fonts.family_name, st.fonts.metrics.cell_w, st.fonts.metrics.cell_h
    );
    scroll_mid(&a, &mut st);
    for s in scenes() {
        if a.only.as_deref().is_some_and(|o| !s.name.contains(o)) {
            continue;
        }
        let mut h = Headless::demo_with(s.name, &[s.file], (100, 30), &s.theme, st.fonts.has_icons);
        st.fit(&mut h, size);
        (s.drive)(&mut h);
        st.feed(&h);
        st.anim.settle();
        let stats = st.render(s.preedit);
        let path = a.out.join(format!("{}.png", s.name));
        match st.save(&path) {
            Ok(()) => println!(
                "{}: {} surfaces, {} glyphs, {} draws",
                path.display(),
                stats.surfaces,
                stats.glyphs,
                stats.draws
            ),
            Err(e) => eprintln!("{}: {e}", path.display()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn p(a: &[&str]) -> Result<Args, String> {
        parse(a.iter().map(ToString::to_string))
    }

    #[test]
    fn gui_15_the_output_directory_is_required_and_has_no_default() {
        let e = p(&[]).err().unwrap();
        assert!(e.contains("OUT_DIR"), "{e}");
        let e = p(&["--time", "--embedded"]).err().unwrap();
        assert!(e.contains("OUT_DIR"), "options are not a directory: {e}");
    }

    #[test]
    fn gui_15_a_directory_and_the_options_parse() {
        let a = p(&["/x/out", "--adapter", "llvmpipe", "--only", "picker", "--time", "--embedded"])
            .unwrap();
        assert_eq!(a.out, PathBuf::from("/x/out"));
        assert_eq!(
            (a.adapter.as_deref(), a.only.as_deref(), a.time, a.embedded),
            (Some("llvmpipe"), Some("picker"), true, true)
        );
        assert!(p(&["/a", "/b"]).is_err(), "one directory");
        assert!(p(&["/a", "--nope"]).is_err());
        assert!(p(&["/a", "--adapter"]).is_err());
    }
}
