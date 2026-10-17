//! COLOR-1: the color budget. One hue has one meaning across all of sim: neutral grays and weight are
//! all structure; green, amber and red are changes and nothing else; blue is the accent, "here,
//! where you act"; magenta is the alert, only for what is hidden or dangerous. The chrome's colors
//! are walked by role (pure) and the screens cell by cell (the cells that are not neutral are the
//! ones that may be).
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, palette};
use sim::color::{Role, Syntax, Tones, to_lch};
use sim::layout::Skin;
use sim::paint::{Look, syntax_hue};
use sim::sim::SimEvent;

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
/// (OKLCH degrees: red 25, amber 85, green 145, blue 250, magenta 320) within 35 degrees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hue {
    Red,
    Amber,
    Green,
    Blue,
    Magenta,
}

fn hue_of(c: Rgb) -> Result<Option<Hue>, f64> {
    let [_, chroma, h] = to_lch(c);
    if chroma < 0.03 {
        return Ok(None);
    }
    let near = |center: f64| {
        let d = (h - center).abs();
        d.min(360.0 - d)
    };
    let all = [
        (Hue::Red, 25.0),
        (Hue::Amber, 85.0),
        (Hue::Green, 145.0),
        (Hue::Blue, 250.0),
        (Hue::Magenta, 320.0),
    ];
    all.into_iter().find(|(_, c)| near(*c) < 35.0).map(|(x, _)| Some(x)).ok_or(h)
}

fn role_of(h: Hue) -> Role {
    match h {
        Hue::Red | Hue::Amber | Hue::Green => Role::Change,
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
fn color_1_the_three_changes_the_accent_and_the_alert_are_five_hues_far_apart() {
    for bg in BGS {
        let t = Tones::new(&palette(bg));
        use sim::color::Chg::{Add, Del, Mod};
        let hues = [
            ("add", t.change_fg(Add)),
            ("mod", t.change_fg(Mod)),
            ("del", t.change_fg(Del)),
            ("accent", t.accent_fg()),
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
    let md = "# Title\n\n## Second\n\n- item one\n> quote\n\n[a link](https://x.y) and `code` and **bold**\n\n```bash\nls -l # c\n```\n";
    let code = "fn a() {\n    // TODO: later\n    if x {\n        y(); \n\t}\n}\n";
    let mut r = rig_env(
        "budget",
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
    let g = r.sim.geo();
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
                            || (y == head && x >= g.ex && ch == '●')
                    }
                    Role::Accent => {
                        (y == last && !normal && (x < 12 || usize::from(x) + 12 > chars.len()))
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
                        y == last
                            && alert
                            && at.is_some_and(|a| usize::from(x) + 2 >= a && usize::from(x) < a + 9)
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
        git(&e.cwd, &["config", "filter.x.clean", "touch witness"]);
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
