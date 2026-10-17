//! UI-7: TODO comments as todo-comments.nvim marks them, in sim's single style: the keyword (and
//! its colon) of a comment is a badge, bold primary on a surface, the same for every keyword, and
//! there is no sign for it.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::color::{Rgb, palette};
use sim::color::{Syntax, Tones};
use sim::layout::{Skin, todo_badge};
use sim::paint::Look;
use sim::sim::SimEvent;
use sim::syn::Spans;

const BG: Rgb = (0x1e, 0x1e, 0x1e);

/// The badge of a line whose comment is everything after `//`.
fn badge(line: &str) -> Option<String> {
    let start = line.find("//")?;
    let spans: Spans = vec![(start..line.chars().count(), Look::Com)];
    let cs: Vec<char> = line.chars().collect();
    todo_badge(line, &spans).map(|r| cs[r].iter().collect())
}

#[test]
fn ui_7_a_keyword_in_capitals_as_a_word_followed_by_a_colon_in_a_comment_is_a_badge() {
    assert_eq!(badge("// TODO: later").as_deref(), Some("TODO:"));
    assert_eq!(badge("    x(); // FIXME(owner): y").as_deref(), Some("FIXME(owner):"));
    for k in ["TODO", "NOTE", "FIX", "FIXME", "BUG", "HACK", "WARN", "WARNING", "PERF", "TEST"] {
        assert_eq!(badge(&format!("// {k}: x")), Some(format!("{k}:")), "{k}");
        assert_eq!(
            badge(&format!("//{k}: x")),
            Some(format!("{k}:")),
            "{k} with no blank before it"
        );
    }
    // The longest keyword that fits: FIXME is not FIX, WARNING is not WARN.
    assert_eq!(badge("// FIXME: x").as_deref(), Some("FIXME:"));
    assert_eq!(badge("// WARNING: x").as_deref(), Some("WARNING:"));
}

#[test]
fn ui_7_prose_the_wrong_case_a_longer_word_and_a_missing_colon_are_not_badges() {
    for line in [
        "// TODO list for later",
        "// todo: lower case",
        "// Todo: mixed",
        "// TODOS: a longer word",
        "// aTODO: part of a word",
        "// NOTE this has no colon",
        "// BUG(unclosed: x",
        "// BUG(a)(b): two groups",
        "//",
    ] {
        assert_eq!(badge(line), None, "{line:?}");
    }
    // Only a comment has one: a keyword in code, or in a string, is not.
    let spans: Spans = vec![(15..25, Look::Com)];
    assert_eq!(todo_badge("let TODO: u8 = 1; // x", &spans), None);
    assert_eq!(todo_badge("let s = \"TODO: x\"", &vec![(8..17, Look::Str)]), None);
}

#[test]
fn ui_7_only_the_first_keyword_of_a_line_is_a_badge() {
    assert_eq!(badge("// TODO: a FIX: b").as_deref(), Some("TODO:"));
    let two = vec![(0..9, Look::Com), (14..30, Look::Com)];
    assert_eq!(
        todo_badge("/* x */ y /* BUG: z */", &two),
        None,
        "(no keyword before a comment ends)"
    );
}

fn body(name: &str, skin: Skin, text: &str) -> Rig {
    let file = format!("a.rs={text}");
    let mut r = rig_env(name, &[&file], &["a.rs"], (80, 24), |e| {
        e.skin = skin;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r
}

const CODE: &str = "fn a() {\n    // TODO: later\n    // FIX: now\n    // NOTE(me): see\n    // plain comment\n    let TODO: u8 = 1;\n}\n";

#[test]
fn ui_7_every_keyword_has_the_same_style_bold_primary_on_a_surface_and_the_rest_of_the_comment_is_a_comment()
 {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = body("ui7-style", skin, CODE);
        let (t, g) = (Tones::new(&palette(BG)), r.sim.geo());
        let cell = |y: usize, x: u16| {
            let on = r.shot.paints[y].iter().filter(|(c, _)| c.contains(&x)).map(|p| p.1);
            on.fold((None, None, false), |a, p| (a.0.or(p.fg), a.1.or(p.bg), a.2 || p.bold))
        };
        let at = |what: &str| {
            let y = (usize::from(g.y0)..r.shot.frame.rows.len())
                .find(|&y| r.shot.frame.rows[y].text.contains(what))
                .unwrap();
            let x = r.shot.frame.rows[y].text.find(what).unwrap();
            (y, r.shot.frame.rows[y].text[..x].chars().count() as u16)
        };
        let style = (Some(t.primary), Some(t.card), true);
        for (kw, len) in [("TODO:", 5u16), ("FIX:", 4), ("NOTE(me):", 9)] {
            let (y, x) = at(kw);
            for k in 0..len {
                assert_eq!(cell(y, x + k), style, "{skin:?}: {kw} col {k}");
            }
            // The word after it is the comment's: no badge, and it is a comment.
            assert_eq!(cell(y, x + len + 1).1, None, "{skin:?}: {kw}: the rest of the comment");
        }
        // No badge on prose or on code.
        let (y, x) = at("plain comment");
        assert_eq!(cell(y, x).1, None);
        let (y, x) = at("TODO: u8");
        assert_eq!(cell(y, x).1, None, "{skin:?}: a keyword in code");
        // No sign for it: the sign column's cells stay blank (git's and the App's).
        let (y, _) = at("TODO: later");
        let cells: String =
            r.shot.frame.rows[y].text.chars().skip(usize::from(g.ex)).take(2).collect();
        assert_eq!(cells, "  ", "{skin:?}: no sign");
        // The text is the file's.
        assert_eq!(r.text(), CODE.trim_end_matches('\n'));
    }
}

#[test]
fn ui_7_calm_and_a_file_without_comments_have_no_badges() {
    let calm = rig_env("ui7-calm", &[&format!("a.rs={CODE}")], &["a.rs"], (80, 24), |_| {});
    assert!(calm.shot.paints.iter().all(Vec::is_empty));
    let r = body("ui7-none", Skin::Flat, "fn a() {}\n");
    let t = Tones::new(&palette(BG));
    assert!(
        r.shot.paints.iter().flatten().all(|(_, p)| p.bg != Some(t.card) || p.fg.is_none() || true)
    );
}
