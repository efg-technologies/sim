//! DARK-1 to DARK-5: the dark board. A look of its own, depth's boxes and surfaces with one rule on
//! top: at rest no cell has a hue, and only a change, where you act and a danger are lit. Every cell
//! is taken as the Painter and a window take it (`paint::Resolver`, GUI-3), not from the paints alone,
//! so that a syntax color or a look's hue counts too.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{Syntax, to_lch};
use sim::layout::Skin;
use sim::paint::Resolver;
use sim::sim::{SimEffect, SimEvent};
use sim_core::core::app::Event;
use sim_core::core::color::Rgb;

const BG: Rgb = (0x1e, 0x1e, 0x1e);
/// The ANSI colors a terminal answers (OSC 4): red, green, yellow, magenta, cyan.
const ANSI: [&str; 5] = [
    "4;1;rgb:cdcd/3131/3131",
    "4;2;rgb:0d0d/bcbc/7979",
    "4;3;rgb:e5e5/e5e5/1010",
    "4;5;rgb:bcbc/3f3f/bcbc",
    "4;6;rgb:1111/a8a8/cdcd",
];
const CODE: &str = "// A comment.\nuse std::fmt;\n\nfn main() {\n    let name = \"world\";\n    let n: u32 = 42;\n    println!(\"hello {name} {n}\");\n}\n";
const MD: &str = "# Title\n\nSome text with a [link](https://example.com/a) and `code`.\n\n## Second\n\n- item\n> quote\n\n```rust\nfn x() {}\n```\n\n### Third\n\nThe end.\n";

/// A clean project (everything committed), `README.md` and `src/main.rs` open, the tree open beside
/// them and the text focused, on the page `BG` with the terminal's ANSI colors, in `skin`.
fn board(name: &str, skin: Skin, extra: &[&str]) -> Rig {
    let (md, code) = (format!("README.md={MD}"), format!("src/main.rs={CODE}"));
    let mut files = vec![md.as_str(), code.as_str()];
    files.extend(extra);
    let mut r = rig_env(name, &files, &["README.md"], (110, 32), |e| {
        git(&e.cwd, &["init", "-q"]);
        git(&e.cwd, &["add", "-A"]);
        git(&e.cwd, &["commit", "-q", "-m", "init"]);
        e.skin = skin;
        e.syn = Syntax::new(true);
        e.chrome = true;
        e.icons = true;
        e.home = e.cwd.parent().map(std::path::Path::to_path_buf);
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    ANSI.iter().for_each(|a| r.send(SimEvent::Osc(a.to_string())));
    r.keys(":e src/main.rs\n<C-n><C-w>l");
    assert!(r.sim.panel.open && !r.sim.panel.focus, "the tree is open and the text has the focus");
    r
}

fn chromatic(c: Rgb) -> bool {
    c.0.abs_diff(c.1) > 2 || c.1.abs_diff(c.2) > 2
}

/// Every cell of the screen whose text color (after the fog) or background has a hue, or that is
/// drawn with SGR 31-36: its column, its row and the color (none for an ANSI color).
fn hues(r: &Rig) -> Vec<(usize, usize, Option<Rgb>)> {
    let mut res = Resolver::new();
    let mut out = vec![];
    for y in 0..r.shot.frame.rows.len() {
        for (x, c) in res.row(&r.shot, y).into_iter().enumerate() {
            let colors = [c.ink(BG, c.fade), c.bg].into_iter().flatten();
            out.extend(colors.filter(|&c| chromatic(c)).map(|c| (x, y, Some(c))));
            out.extend(c.ansi.map(|_| (x, y, None)));
        }
    }
    out
}

fn dark_at_rest(r: &Rig, what: &str) {
    let lit = hues(r);
    assert!(
        lit.is_empty(),
        "{what}: {} cells have a hue, the first at {:?} on the row {:?}\n{}",
        lit.len(),
        lit[0],
        r.shot.frame.rows[lit[0].1].text,
        dump(&r.shot.frame)
    );
}

/// The hue families a screen lights: change (red, amber, green), act (blue), danger (magenta), and
/// what is none of them (cyan, the kind, which dark never lights).
fn families(r: &Rig) -> std::collections::BTreeSet<&'static str> {
    let family = |c: Option<Rgb>| {
        let Some(c) = c else { return "ansi" };
        let h = to_lch(c)[2].rem_euclid(360.0);
        let near = |x: f64| (h - x).abs().min(360.0 - (h - x).abs());
        let all = [
            ("change", 25.0),
            ("change", 85.0),
            ("change", 145.0),
            ("kind", 195.0),
            ("act", 250.0),
            ("danger", 320.0),
        ];
        all.into_iter().min_by(|a, b| near(a.1).total_cmp(&near(b.1))).unwrap().0
    };
    hues(r).into_iter().map(|(_, _, c)| family(c)).collect()
}

fn lights(r: &Rig, what: &str, family: &str) {
    let f = families(r);
    assert_eq!(f.into_iter().collect::<Vec<_>>(), [family], "{what}\n{}", dump(&r.shot.frame));
}

// ---- DARK-1 ----

#[test]
fn dark_1_zl_goes_round_calm_flat_depth_dark_and_the_choice_is_remembered() {
    let mut r =
        rig_env("dark1-cycle", &["a.txt=one\n"], &["a.txt"], (80, 24), |e| e.skin = Skin::Calm);
    for (skin, word) in
        [(Skin::Flat, "flat"), (Skin::Depth, "depth"), (Skin::Dark, "dark"), (Skin::Calm, "calm")]
    {
        r.keys("zl");
        assert_eq!((r.sim.skin, r.status()), (skin, format!("look: {word}").as_str()));
    }
    let asked: Vec<Skin> = r
        .log
        .iter()
        .filter_map(|e| if let SimEffect::Look(s) = e { Some(*s) } else { None })
        .collect();
    assert_eq!(asked, [Skin::Flat, Skin::Depth, Skin::Dark, Skin::Calm]);
    // remembered as the others are, in LOOK-1's file; the first run is still depth
    let dir = temp_dir("dark1-state");
    let state = dir.0.join("sim");
    assert_eq!(sim::io::load_look(&state), Skin::Depth, "the default stays depth");
    sim::io::save_look(&state, Skin::Dark).unwrap();
    assert_eq!(std::fs::read_to_string(state.join("look")).unwrap(), "dark\n");
    assert_eq!(sim::io::load_look(&state), Skin::Dark);
    assert_eq!(Skin::parse("dark"), Some(Skin::Dark));
}

#[test]
fn dark_1_without_a_palette_without_truecolor_and_with_no_color_dark_is_depth() {
    let fallbacks: [(&str, Option<Rgb>, Syntax); 3] = [
        ("nopal", None, Syntax::new(true)),
        ("notrue", Some(BG), Syntax::new(false)),
        ("mono", Some(BG), Syntax::mono()),
    ];
    for (name, bg, syn) in fallbacks {
        let shot = |skin: Skin| {
            let files = [&*format!("README.md={MD}"), &*format!("a.rs={CODE}")];
            let mut r = rig_env(&format!("dark1-{name}"), &files, &["README.md"], (90, 24), |e| {
                e.skin = skin;
                e.syn = syn;
                e.chrome = true;
            });
            if let Some(bg) = bg {
                r.send(SimEvent::Term(Event::Bg(bg)));
            }
            r.keys(":e a.rs\n");
            assert_eq!(r.sim.skin, skin);
            r.shot
        };
        assert_eq!(shot(Skin::Dark), shot(Skin::Depth), "{name}: dark is what depth is there");
    }
}

// ---- DARK-2 ----

#[test]
fn dark_2_at_rest_no_cell_has_a_hue_and_depth_on_the_same_screen_has_some() {
    let mut depth = board("dark2-depth", Skin::Depth, &[]);
    assert!(
        !hues(&depth).is_empty(),
        "depth: the syntax, the link, the heading and the tab are lit"
    );
    depth.keys(":e README.md\n");
    assert!(families(&depth).contains("kind"), "depth: a heading is cyan");
    let mut r = board("dark2-dark", Skin::Dark, &[]);
    assert!(r.sim.geo().boxed, "depth's boxes");
    dark_at_rest(&r, "the code");
    // a keyword is bold, a comment the secondary gray: weight and lightness say what the hue said
    let mut res = Resolver::new();
    let y = (0..r.shot.frame.rows.len())
        .find(|&y| r.shot.frame.rows[y].text.contains("fn main()"))
        .unwrap();
    let x = r.shot.frame.rows[y].text.chars().position(|c| c == 'f').unwrap();
    assert!(res.row(&r.shot, y)[x].attrs.bold(), "the keyword `fn` is bold");
    r.keys(":e README.md\n");
    dark_at_rest(&r, "the markdown");
    // the read view: the link is drawn as its text (MD-12), underlined in the text's gray
    r.keys("<Space>um");
    r.keys("j");
    dark_at_rest(&r, "the read view");
    let y = (0..r.shot.frame.rows.len())
        .find(|&y| r.shot.frame.rows[y].text.contains("with a link and"))
        .unwrap();
    let x = r.shot.frame.rows[y].text.chars().collect::<Vec<_>>();
    let at = (0..x.len()).find(|&i| x[i..].starts_with(&['l', 'i', 'n', 'k'])).unwrap();
    let cell = res.row(&r.shot, y)[at];
    assert!(cell.attrs.underline(), "a link is underlined");
    // the tree's own keys, the panel focused: the TREE block is the accent's (an act), and Esc
    // gives the focus back: dark again
    r.keys("<C-w>h");
    lights(&r, "the tree has the focus", "act");
    r.keys("<C-w>l");
    dark_at_rest(&r, "the text again");
}

// ---- DARK-3 ----

#[test]
fn dark_3_a_change_an_act_and_a_danger_light_each_in_its_own_hue_and_nothing_else_does() {
    // a line changed against HEAD: the sign, the scrollbar's mark, the bar's counts
    let mut r = board("dark3-head", Skin::Dark, &[]);
    r.keys("jx:w\nj");
    assert!(!r.sim.bufs[r.sim.cur].app.buf.dirty());
    lights(&r, "a line that differs from HEAD", "change");
    // a change from the disk: its marks and the flash of the write
    let mut r = board("dark3-disk", Skin::Dark, &[]);
    let id = r.sim.bufs[r.sim.cur].id;
    r.send(SimEvent::App(id, disk(CODE.replace("42", "43").as_bytes())));
    lights(&r, "a change from the disk", "change");
    // Insert mode: the mode's block and the position's
    let mut r = board("dark3-insert", Skin::Dark, &[]);
    r.keys("i");
    lights(&r, "Insert mode", "act");
    // a search: its matches
    let mut r = board("dark3-search", Skin::Dark, &[]);
    r.keys("/name\n");
    lights(&r, "a search", "act");
    // a char shown in another form: the alert
    let mut r = board("dark3-hidden", Skin::Dark, &["h.txt=a\u{202e}b\n"]);
    r.keys(":e h.txt\n");
    lights(&r, "U+202E", "danger");
    // a link that lies (MD-10)
    let mut r = board(
        "dark3-lie",
        Skin::Dark,
        &["l.md=[https://good.example](https://evil.example)\n\nnext\n"],
    );
    r.keys(":e l.md\nj");
    lights(&r, "a link that lies", "danger");
}

// ---- DARK-4 ----

#[test]
fn dark_4_a_light_is_put_out_and_the_board_is_dark_again() {
    let mut r = board("dark4", Skin::Dark, &[]);
    dark_at_rest(&r, "at rest");
    r.keys("i");
    assert!(!hues(&r).is_empty());
    r.keys("<Esc>");
    dark_at_rest(&r, "after Esc");
    r.keys("/name\n");
    assert!(!hues(&r).is_empty());
    r.keys(":noh\n");
    dark_at_rest(&r, "after :noh");
    // a change from the disk: lit until it has been seen (`:seen`) and its flash is over
    let id = r.sim.bufs[r.sim.cur].id;
    r.send(SimEvent::App(id, disk(CODE.replace("42", "43").as_bytes())));
    assert!(!hues(&r).is_empty());
    let flashes: Vec<u64> = r
        .log
        .iter()
        .filter_map(|e| if let SimEffect::Flash(n, _) = e { Some(*n) } else { None })
        .collect();
    flashes.into_iter().for_each(|n| r.send(SimEvent::Unflash(n)));
    r.keys(":seen\n");
    dark_at_rest(&r, "after :seen");
    // and by the cursor entering the marked line (DISK-6)
    r.send(SimEvent::App(id, disk(CODE.replace("42", "44").as_bytes())));
    let flashes: Vec<u64> = r
        .log
        .iter()
        .filter_map(|e| if let SimEffect::Flash(n, _) = e { Some(*n) } else { None })
        .collect();
    flashes.into_iter().for_each(|n| r.send(SimEvent::Unflash(n)));
    assert!(!hues(&r).is_empty());
    r.keys("]c");
    r.keys("j");
    dark_at_rest(&r, "after the cursor went through the mark");
}

// ---- DARK-5 ----

#[test]
fn dark_5_the_bar_says_only_what_is_not_the_usual_and_nothing_moves_with_the_mode() {
    let mut r = board("dark5", Skin::Dark, &[]);
    let bar = |r: &Rig| r.shot.frame.rows.last().unwrap().text.clone();
    let normal = bar(&r);
    assert!(!normal.contains("NORMAL"), "no block and no word for Normal mode: {normal:?}");
    assert!(normal.contains("master") || normal.contains("main"), "the branch: {normal:?}");
    assert!(normal.contains("1:1"), "the position: {normal:?}");
    for word in ["utf-8", "LF", "rust"] {
        assert!(!normal.contains(word), "{word} is the usual: {normal:?}");
    }
    r.keys("i");
    let insert = bar(&r);
    assert!(insert.contains("INSERT"), "{insert:?}");
    // the branch and the position are where they were: the block's cells were kept
    let col = |s: &str, w: &str| s.find(w).map(|b| s[..b].chars().count());
    let branch = |s: &str| col(s, "master").or(col(s, "main"));
    assert_eq!(branch(&normal), branch(&insert), "{normal:?}\n{insert:?}");
    assert_eq!(col(&normal, "1:1"), col(&insert, "1:1"), "{normal:?}\n{insert:?}");
    assert_eq!(normal.chars().count(), insert.chars().count());
    // depth keeps its gray NORMAL block
    let depth = board("dark5-depth", Skin::Depth, &[]);
    assert!(bar(&depth).contains("NORMAL"));
}
