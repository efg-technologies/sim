//! COLOR-1: the color budget. One hue has one meaning across all of sim: neutral grays and weight are
//! all structure; green, amber and red are changes and nothing else; blue is the accent, "here,
//! where you act"; magenta is the alert, only for what is hidden or dangerous; cyan is the kind,
//! what a heading is (hue says what a thing is, lightness and weight how much). The chrome's colors
//! are walked by role (pure) and the screens cell by cell (the cells that are not neutral are the
//! ones that may be).
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{Role, Syntax, Tones, to_lch};
use sim::layout::Skin;
use sim::paint::{Look, syntax_hue};
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, palette};

const BG: Rgb = (0x1e, 0x1e, 0x1e);
const BGS: [Rgb; 8] = [
    (0x00, 0x00, 0x00),
    (0x1e, 0x1e, 0x1e),
    (0x28, 0x2c, 0x34),
    (0x00, 0x2b, 0x36),
    (0xff, 0xff, 0xff),
    (0xfa, 0xfa, 0xfa),
    (0xfd, 0xf6, 0xe3),
    (0xef, 0xf1, 0xf5),
];

/// The hue class of a color: none for a gray, else the nearest of the five hues of the interface
/// (OKLCH degrees: red 25, amber 85, green 145, cyan 195, blue 250, magenta 320) within 35 degrees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hue {
    Red,
    Amber,
    Green,
    Cyan,
    Blue,
    Magenta,
}

fn hue_of(c: Rgb) -> Result<Option<Hue>, f64> {
    let [_, chroma, h] = to_lch(c);
    if chroma < 0.03 {
        return Ok(None);
    }
    let near = |center: f64| {
        let d = (h - center).rem_euclid(360.0);
        d.min(360.0 - d)
    };
    let all = [
        (Hue::Red, 25.0),
        (Hue::Amber, 85.0),
        (Hue::Green, 145.0),
        (Hue::Cyan, 195.0),
        (Hue::Blue, 250.0),
        (Hue::Magenta, 320.0),
    ];
    // the nearest of them (green's and cyan's 35 degrees overlap)
    let nearest = all.into_iter().min_by(|a, b| near(a.1).total_cmp(&near(b.1)));
    nearest.filter(|(_, c)| near(*c) < 35.0).map(|(x, _)| Some(x)).ok_or(h)
}

/// Whether column `x` of a row of text is in a token that stands for a hidden char: `^` and the
/// char after it, or `<U+` and the hex digits to the `>`.
fn in_token(text: &str, x: usize) -> bool {
    let cs: Vec<char> = text.chars().collect();
    let on = |from: usize, len: usize| (from..from + len).contains(&x);
    (0..cs.len()).any(|i| match cs[i] {
        '^' => {
            cs.get(i + 1).is_some_and(|c| "@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_?".contains(*c))
                && on(i, 2)
        }
        '<' => {
            let tok: String = cs[i..].iter().take(8).collect();
            tok.starts_with("<U+") && tok.ends_with('>') && on(i, 8)
        }
        _ => false,
    })
}

fn role_of(h: Hue) -> Role {
    match h {
        Hue::Red | Hue::Amber | Hue::Green => Role::Change,
        Hue::Cyan => Role::Kind,
        Hue::Blue => Role::Accent,
        Hue::Magenta => Role::Alert,
    }
}

#[test]
fn color_1_every_color_of_the_chrome_has_one_role_and_its_hue_belongs_to_it() {
    for bg in BGS {
        let t = Tones::new(&palette(bg));
        // (a page that has a tint of its own, solarized's, tints its surfaces: no more than that)
        let page = to_lch(bg)[1];
        for (name, role, c) in t.roles() {
            match (role, hue_of(c)) {
                (Role::Neutral, _) if to_lch(c)[1] <= page.max(0.03) + 0.01 => {}
                (Role::Neutral, Ok(h)) => panic!(
                    "{bg:?}: {name} {c:?} is neutral, so it has no hue of its own ({h:?}), page chroma {page:.3}"
                ),
                // (a tint is the page mixed with its hue: on a page with a tint of its own it drifts)
                // (a band is the page mixed toward the heading's hue: likewise)
                (Role::Kind, _) if name.ends_with("band") => {
                    let [_, chroma, h] = to_lch(c);
                    let near = |x: f64| (h - x).abs().min(360.0 - (h - x).abs());
                    assert!(chroma < 0.03 || near(195.0) < 70.0, "{bg:?}: {name} {c:?} hue {h:.0}");
                }
                (Role::Change, _) if name.ends_with("tint") => {
                    // (a pale tint of a pale page may have no chroma to speak of)
                    let [_, chroma, h] = to_lch(c);
                    let near = |x: f64| (h - x).abs().min(360.0 - (h - x).abs());
                    assert!(
                        chroma < 0.03 || near(145.0) < 70.0 || near(25.0) < 70.0,
                        "{bg:?}: {name} {c:?} hue {h:.0}"
                    );
                }
                (_, Ok(None)) => assert!(
                    matches!(role, Role::Change | Role::Accent | Role::Alert)
                        && name.ends_with("text"),
                    "{bg:?}: {name} {c:?} is a gray: only the text on a block may be"
                ),
                (r, Ok(Some(h))) => assert_eq!(
                    role_of(h),
                    r,
                    "{bg:?}: {name} {c:?} is {h:?}, which is {:?}'s, not {r:?}'s",
                    role_of(h)
                ),
                (_, Err(h)) => {
                    panic!("{bg:?}: {name} {c:?} has hue {h:.0}, which is none of the interface's")
                }
            }
        }
    }
}

#[test]
fn color_1_the_three_changes_the_accent_the_kind_and_the_alert_are_six_hues_far_apart() {
    for bg in BGS {
        let t = Tones::new(&palette(bg));
        use sim::color::Chg::{Add, Del, Mod};
        let hues = [
            ("add", t.change_fg(Add)),
            ("mod", t.change_fg(Mod)),
            ("del", t.change_fg(Del)),
            ("accent", t.accent_fg()),
            ("kind", t.head),
            ("alert", t.alert_fg()),
        ]
        .map(|(n, c)| (n, to_lch(c)[2]));
        for (i, (a, ha)) in hues.iter().enumerate() {
            for (b, hb) in &hues[i + 1..] {
                let d = (ha - hb).abs();
                let d = d.min(360.0 - d);
                assert!(d >= 40.0, "{bg:?}: {a} and {b} are {d:.0} degrees apart");
            }
        }
    }
}

#[test]
fn color_1_only_the_looks_of_code_have_a_hue_of_their_own_and_markdown_is_weight_and_gray() {
    use Look::*;
    for look in [Kw, Str, Ty, Num, Fun] {
        assert!(syntax_hue(look).is_some(), "{look:?} is code");
    }
    for look in [
        Com, Tick, Lang, Mark, Url, Link, Code, H1, H2, H3, H4, H5, H6, Bullet, Quote, Em, Band,
        Bold, Sel,
    ] {
        assert_eq!(
            syntax_hue(look),
            None,
            "{look:?} has no hue: a heading is bold in every level, a bullet and a quote's bar are secondary, a link is underlined"
        );
    }
}

// ---- the screens, cell by cell ----

fn rig(skin: Skin) -> Rig {
    rig_named("budget", skin)
}

fn rig_named(name: &str, skin: Skin) -> Rig {
    let md = "# Title\n\n## Second\n\n- item one\n> quote\n\n[a link](https://x.y) and `code` and **bold**\n\n```bash\nls -l # c\n```\n\n> [!NOTE]\n> n\n\n> [!WARNING]\n> w\n\n> [!CAUTION]\n> c\n\n> [!TIP]\n> t\n\n> [!IMPORTANT]\n> i\n";
    let code = "fn a() {\n    // TODO: later\n    if x {\n        y(); \n\t}\n}\n";
    let mut r = rig_env(
        name,
        &[&format!("README.md={md}"), &format!("a.rs={code}"), "b.rs", "src/c.rs", "old/gone.rs"],
        &["README.md"],
        (120, 30),
        |e| {
            git(&e.cwd, &["init", "-q"]);
            git(&e.cwd, &["add", "-A"]);
            git(&e.cwd, &["commit", "-q", "-m", "init"]);
            std::fs::write(e.cwd.join("b.rs"), "changed\n").unwrap();
            make(&e.cwd, &["new.rs"]);
            std::fs::remove_file(e.cwd.join("old/gone.rs")).unwrap();
            git(&e.cwd, &["mv", "src/c.rs", "src/d.rs"]);
            e.skin = skin;
            e.syn = Syntax::new(true);
            e.chrome = true;
            e.icons = true;
            e.home = e.cwd.parent().map(std::path::Path::to_path_buf);
        },
    );
    r.send(SimEvent::Term(Event::Bg(BG)));
    r.keys(":e a.rs\n:e b.rs\n:e README.md\n");
    r
}

/// The merged paint of a cell: each field from the first range that has one.
fn cell(r: &Rig, y: usize, x: u16) -> (Option<Rgb>, Option<Rgb>) {
    let on = r.shot.paints[y].iter().filter(|(c, _)| c.contains(&x)).map(|p| p.1);
    on.fold((None, None), |a, p| (a.0.or(p.fg), a.1.or(p.bg)))
}

/// Which of the screen's cells have a hue, and whether each is where its role may be. `normal`:
/// the mode is Normal (the mode's block is gray, so the accent has no place in the status line);
/// `alert`: the security slot says something that is an alert.
fn check(r: &Rig, what: &str, normal: bool, alert: bool) {
    walk(r, what, normal, alert, false);
}

/// `check`, and with `lit`: the matches of a search are lit in the text (UI-14), so the accent is
/// there too. With a picker open (PICK-1) the paints of the cells of its window may be the accent's
/// and no other hue's (the syntax colors of its preview are looks, SYN-1, and not paints).
fn walk(r: &Rig, what: &str, normal: bool, alert: bool, lit: bool) {
    let g = r.sim.geo();
    let win = r.win();
    let rows = &r.shot.frame.rows;
    let (last, head) = (rows.len() - 1, usize::from(g.y0) - 1);
    let mut seen = std::collections::BTreeSet::new();
    for y in 0..rows.len() {
        let chars: Vec<char> = rows[y].text.chars().collect();
        for x in 0..chars.len() as u16 {
            let (fg, bg) = cell(r, y, x);
            let ch = chars[usize::from(x)];
            // (a color on the foreground of a blank is not seen)
            let colors = [fg.filter(|_| ch != ' '), bg];
            for c in colors.into_iter().flatten() {
                let hue = hue_of(c);
                assert!(
                    hue.is_ok(),
                    "{what}: ({x},{y}) {c:?} has hue {:.0}, none of the interface's",
                    hue.unwrap_err()
                );
                let Some(h) = hue.unwrap() else { continue };
                seen.insert(h as u8);
                let in_window = win.is_some_and(|w| {
                    x >= w.x && x < w.x + w.w && y >= usize::from(w.y) && y < usize::from(w.y + w.h)
                });
                if in_window {
                    assert!(
                        role_of(h) == Role::Accent,
                        "{what}: {c:?} {h:?} on {ch:?} at ({x},{y}) is in the picker's window, where only the accent has a hue"
                    );
                    continue;
                }
                let body = y >= usize::from(g.y0) && y < last;
                let in_panel = body && x >= g.px && x < g.px + g.pw;
                let in_sign = body && x >= g.ex && x < g.tx();
                let ruler = body && x >= g.tx() + g.ew;
                let in_text = body && x >= g.tx() && x < g.tx() + g.ew;
                let flagged = rows[y]
                    .text
                    .chars()
                    .nth(usize::from(g.tx()).saturating_sub(1))
                    .is_some_and(|c| c != ' ');
                let link = r.shot.looks[y].iter().any(|(c, l)| c.contains(&x) && *l == Look::Link);
                let ok = match role_of(h) {
                    Role::Change => {
                        in_panel
                            || in_sign
                            || ruler
                            || (in_text && flagged && c == bg.unwrap_or((1, 2, 3)))
                            || (y == last && "+~-0123456789".contains(ch))
                            || (y == head && x >= g.ex && (ch == '↯' || ch == '●'))
                    }
                    Role::Accent => {
                        (lit && in_text && c == bg.unwrap_or((1, 2, 3)))
                            || (y == last
                                && !normal
                                && (x < 12 || usize::from(x) + 12 > chars.len()))
                            || (y == head && x >= g.ex && ch == '▎')
                            || (in_text && link)
                            || ruler
                    }
                    Role::Alert => {
                        // the shield and the word, in the slot of the status line
                        let at = rows[last]
                            .text
                            .find("untrusted")
                            .map(|b| rows[last].text[..b].chars().count());
                        let slot = y == last
                            && alert
                            && at
                                .is_some_and(|a| usize::from(x) + 2 >= a && usize::from(x) < a + 9);
                        // or a hidden char of the text (UI-15): the cells of a `^X` or `<U+XXXX>` token
                        slot || (in_text && in_token(&rows[y].text, usize::from(x)))
                    }
                    // a heading: its text and its band, in the rows of the text that have one
                    Role::Kind => {
                        in_text
                            && r.shot.looks[y].iter().any(|(_, l)| {
                                matches!(l, Look::H1 | Look::H2 | Look::H3 | Look::Sub)
                            })
                    }
                    Role::Neutral => true,
                };
                assert!(
                    ok,
                    "{what}: {:?} {h:?} on {ch:?} at ({x},{y}) is not where {:?} may be",
                    c,
                    role_of(h)
                );
            }
        }
    }
    eprintln!("{what}: hues seen {seen:?}");
}

#[test]
fn color_1_on_the_screen_a_hue_is_only_where_its_meaning_is() {
    for skin in [Skin::Flat, Skin::Depth] {
        let mut r = rig(skin);
        check(&r, &format!("{skin:?} the text"), true, false);
        // The tree, with every status: names, letters and dots are changes.
        r.keys("<C-n>");
        r.keys("Gl"); // some of the tree open
        check(&r, &format!("{skin:?} the tree"), false, false);
        r.keys("<Esc>");
        // Insert and Visual mode: the mode's block and the position's are the accent's.
        r.keys("i");
        check(&r, &format!("{skin:?} insert"), false, false);
        r.keys("<Esc>v");
        check(&r, &format!("{skin:?} visual"), false, false);
        r.keys("<Esc>");
        // A file modified, one changed on disk, a server's counts, an untrusted root.
        let id = r.sim.bufs[1].id;
        r.send(SimEvent::App(id, disk(b"on disk\n")));
        r.sim.bufs[0].diag = (2, 1);
        r.sim.trust = Some(false);
        r.keys("x");
        check(&r, &format!("{skin:?} tabs and the slot"), true, true);
        // A trusted root has no alert in it.
        r.sim.trust = Some(true);
        r.keys("j");
        check(&r, &format!("{skin:?} trusted"), true, false);
    }
}

#[test]
fn color_1_a_git_that_was_not_run_for_safety_says_so_in_the_alert_s_color() {
    let mut r = rig_env("budget-skip", &["a.rs"], &["a.rs"], (100, 12), |e| {
        git(&e.cwd, &["init", "-q"]);
        git(&e.cwd, &["config", "core.sshCommand", "touch witness"]);
        e.skin = Skin::Flat;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    let t = Tones::new(&palette(BG));
    assert!(r.status().starts_with("git skipped"), "{}", r.status());
    let y = r.shot.frame.rows.len() - 1;
    assert_eq!(cell(&r, y, 3).0, Some(t.alert_fg()), "magenta");
    r.keys("j");
    assert_ne!(
        cell(&r, y, 3).0,
        Some(t.alert_fg()),
        "gone with the message: the bar after it has no alert"
    );
}

#[test]
fn color_1_the_picker_and_a_lit_search_use_only_neutral_surfaces_and_the_accent() {
    for skin in [Skin::Flat, Skin::Depth] {
        // the search of contents: a markdown file and a file of code in the preview
        let mut r = rig_named(&format!("budget-pick-{skin:?}"), skin);
        r.keys("<C-f>item");
        assert!(r.win().is_some());
        walk(&r, &format!("{skin:?} grep, markdown"), true, false, false);
        r.keys("<C-u>fn");
        r.keys("<C-n>");
        walk(&r, &format!("{skin:?} grep, code"), true, false, false);
        // the file finder, with the selection moved
        r.keys("<Esc><C-p>rs");
        r.keys("<C-n>");
        walk(&r, &format!("{skin:?} finder"), true, false, false);
        r.keys("<C-u>");
        walk(&r, &format!("{skin:?} finder, no query"), true, false, false);
        r.keys("zzzz");
        walk(&r, &format!("{skin:?} finder, no match"), true, false, false);
        // a search lit in the text, the current match and the others, and put out again
        r.keys("<Esc>/item\n");
        walk(&r, &format!("{skin:?} lit"), true, false, true);
        r.keys("n");
        walk(&r, &format!("{skin:?} lit, next"), true, false, true);
        r.keys(":noh\n");
        walk(&r, &format!("{skin:?} put out"), true, false, false);
    }
}

#[test]
fn color_1_no_look_of_the_text_is_magenta_the_alert_s_hue() {
    use Look::*;
    // every look there is, with the color the Painter gives it where it has a palette (the syntax
    // hues of `Syntax::hue`, derived or answered by the terminal) and where it has none (SGR 31-36)
    let all = [
        Com, Str, Kw, Num, Ty, Fun, Tick, Lang, Mark, Url, Link, Code, H1, H2, H3, H4, H5, H6, Sub,
        Bullet, Quote, Em, Band, Bold, Sel,
    ];
    for bg in BGS {
        for answered in [false, true] {
            let mut syn = Syntax::new(true);
            syn.bg(bg);
            if answered {
                // the terminal's own magenta (the alert's) answers too, and no look takes it
                syn.reply("4;5;rgb:ff/00/ff");
                syn.reply("4;2;rgb:00/ff/00");
            }
            for look in all {
                let Some(c) = syn.hue(look) else { continue };
                assert_ne!(hue_of(c), Ok(Some(Hue::Magenta)), "{bg:?} {look:?} {c:?} is magenta");
            }
        }
    }
    for look in all {
        assert_ne!(
            sim::paint::ansi_of(look),
            Some(5),
            "{look:?} is SGR 35, magenta, without a palette"
        );
    }
    // numbers and constants are the strings' green, whatever the terminal says
    let mut syn = Syntax::new(true);
    syn.bg(BG);
    assert_eq!(syn.hue(Num), syn.hue(Str));
}

#[test]
fn color_1_the_hidden_chars_of_a_file_are_the_only_alert_in_the_text() {
    for skin in [Skin::Flat, Skin::Depth] {
        let file = "h.txt=a\u{1b}b \u{202e}c <U+202E> ^[ end\nsecond\n";
        let mut r = rig_env(&format!("budget-hid-{skin:?}"), &[file], &["h.txt"], (100, 20), |e| {
            e.skin = skin;
            e.syn = Syntax::new(true);
            e.chrome = true;
        });
        r.send(SimEvent::Term(Event::Bg(BG)));
        walk(&r, &format!("{skin:?} hidden chars"), true, false, false);
        let t = Tones::new(&palette(BG));
        let (back, _) = t.alert_block();
        let blocks = r.shot.paints.iter().flatten().filter(|(_, p)| p.bg == Some(back) && p.sealed);
        assert!(blocks.count() >= 2, "{skin:?}: both tokens are drawn as the alert's block");
    }
}
