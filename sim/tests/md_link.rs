//! MD-10: a link whose text looks like an address that its target is not is marked. The judgment
//! (pure), which links of a line it is made of, and the screens: the target is the alert's block in
//! a line of the read view and in a raw one, inverse and bold with no color, dim and on the screen
//! in calm, and nothing of the link is ever left out.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{Syntax, Tones};
use sim::layout::Skin;
use sim::md::{deceptive, deceptive_targets};
use sim::paint::{Painter, Screen};
use sim::sim::SimEvent;
use sim::syn::{Spans, Syn};
use sim_core::core::app::Event;
use sim_core::core::color::{Rgb, palette};

const BG: Rgb = (0x1e, 0x1e, 0x1e);

#[test]
fn md_10_a_text_that_is_an_address_other_than_the_target_is_deceptive() {
    let lies = [
        ("https://good.example/docs", "https://evil.example/login"),
        ("good.example", "https://evil.example"),
        ("www.good.example", "https://evil.example"),
        ("docs.example.org/guide", "https://example.org/guide"),
        ("example.com", "https://example.com.evil.example/"),
        ("https://good.example", "https://good.example@evil.example/"),
        ("https://good.example", "//evil.example"),
        ("example.com", "/redirect?to=evil"),
        ("https://good.example", "javascript:alert(1)"),
        ("good.example", "data:text/html;base64,AAAA"),
        ("good.example", "VBScript:run"),
        ("/etc/hosts", "https://evil.example/etc/passwd"),
        ("/etc/passwd", "https://evil.example/passwd"),
        ("~/notes.md", "https://evil.example/home/notes.md"),
        ("docs/guide/intro.md", "https://example.com/intro.md"),
        ("./src/main.rs", "../other/file.rs"),
        ("src/main.rs", "https://github.com/x/y/blob/main/src/lib.rs"),
        ("**https://good.example**", "https://evil.example"),
        ("`good.example`", "https://evil.example"),
        ("HTTPS://GOOD.EXAMPLE", "https://evil.example"),
    ];
    for (text, target) in lies {
        assert!(deceptive(text, target), "[{text}]({target}) is deceptive");
    }
}

#[test]
fn md_10_the_same_address_a_name_and_a_phrase_are_not() {
    let honest = [
        ("docs", "https://example.com/docs"),
        ("click here", "https://evil.example"),
        ("Node.js", "https://nodejs.org"),
        ("main.rs", "https://example.com/src/main.rs"),
        ("README.md", "README.md"),
        ("Cargo.toml", "https://github.com/x/y/blob/main/Cargo.toml"),
        ("v1.2.3", "https://example.com/releases/1.2.3"),
        ("and/or", "https://example.com/logic"),
        ("e.g.", "https://example.com/eg"),
        ("example.com", "https://example.com/"),
        ("example.com", "https://www.example.com"),
        ("www.example.com", "http://example.com:8080/x"),
        ("https://example.com/a", "https://example.com/b"),
        ("https://example.com", "https://user@example.com/"),
        ("example.com/docs", "https://example.com/docs/intro"),
        ("src/main.rs", "../src/main.rs"),
        ("src/main.rs", "https://github.com/x/y/blob/main/src/main.rs"),
        ("./src/main.rs", "src/main.rs"),
        ("/etc/hosts", "/etc/hosts"),
        ("~/notes.md", "notes.md"),
        ("mail me", "mailto:me@example.com"),
        ("me@example.com", "mailto:me@example.com"),
        ("", "https://example.com"),
        ("https://example.com", ""),
    ];
    for (text, target) in honest {
        assert!(!deceptive(text, target), "[{text}]({target}) is not deceptive");
    }
}

fn spans_of(line: &str) -> (Vec<char>, Spans) {
    let lines = [line.to_string()];
    let mut syn = Syn::new("a.md", "").unwrap();
    (line.chars().collect(), syn.spans(&|i| lines.get(i).cloned(), 0, 1).remove(0))
}

/// The targets of the line that are marked, as text.
fn marked(line: &str) -> Vec<String> {
    let (cs, sp) = spans_of(line);
    deceptive_targets(&cs, &sp).into_iter().map(|r| cs[r].iter().collect()).collect()
}

#[test]
fn md_10_the_targets_of_the_inline_links_of_a_line_that_are_deceptive_are_found() {
    let line = "see [https://a.example/x](https://b.example/y) and [docs](https://c.example) and \
                [d.example](https://e.example)";
    assert_eq!(marked(line), ["https://b.example/y", "https://e.example"]);
    // markers and code in the text; a title after the target
    assert_eq!(
        marked("[**https://a.example**](https://b.example \"t\")"),
        ["https://b.example"],
        "the markers of the text are not the text, the title is not the target"
    );
    assert_eq!(marked("[`a.example`](https://b.example)"), ["https://b.example"]);
    assert_eq!(marked("[https://a.example](<https://b.example>)"), ["<https://b.example>"]);
    assert_eq!(
        marked("[a.example](https://b.example/x_(y)) then [c.example](https://d.example)"),
        ["https://b.example/x_(y)", "https://d.example"],
        "a parenthesis in the target is part of it, and the next link is found"
    );
    // an image's text is not clicked, a reference's target is a line of its own, an autolink has no text
    assert_eq!(marked("![https://a.example](https://b.example/i.png)"), Vec::<String>::new());
    assert_eq!(marked("[https://a.example][ref] <https://b.example>"), Vec::<String>::new());
    assert_eq!(
        marked("no link here, only https://a.example and (https://b.example)"),
        Vec::<String>::new()
    );
    assert_eq!(marked(""), Vec::<String>::new());
}

fn draw(s: Screen) -> String {
    let mut p = Painter::new(Vec::new());
    p.draw(s).unwrap();
    String::from_utf8(p.out).unwrap()
}

/// The SGR sequence in force at the first `what` of `out`.
fn seq(out: &str, what: &str) -> String {
    assert!(out.contains(what), "{what:?} is not drawn in {out:?}");
    let i = out.find(what).unwrap();
    format!("\x1b[{}", out[..i].rsplit("\x1b[").next().unwrap())
}

fn shown(name: &str, skin: Skin, mono: bool, text: &str) -> Rig {
    let mut r = rig_env(name, &[&format!("a.md={text}")], &["a.md"], (80, 12), |e| {
        e.skin = skin;
        e.chrome = true;
        e.md = true;
        e.syn = if mono { Syntax::mono() } else { Syntax::new(true) };
    });
    r.send(SimEvent::Term(Event::Bg(BG)));
    r.deliver_spans();
    r
}

/// A file whose first line is where the cursor is (so it is raw) and whose third line is read.
const LINKS: &str = "[https://good.example/docs](https://evil.example/login) raw\n\n\
                     [https://good.example/docs](https://evil.example/login) **read**\n\
                     [docs](https://fine.example/docs) and [fine.example](https://fine.example/x)\n";

/// The alert's block as flat and depth write it: its own text on its surface, nothing else.
fn block() -> String {
    let t = Tones::new(&palette(BG));
    let (back, text) = t.alert_block();
    format!("\x1b[0;38;2;{};{};{};48;2;{};{};{}m", text.0, text.1, text.2, back.0, back.1, back.2)
}

#[test]
fn md_10_the_target_is_the_alert_s_block_in_a_raw_line_and_in_a_read_one() {
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md10-block", skin, false, LINKS);
        let out = draw(r.shot.clone());
        let hits: Vec<_> = out.match_indices("https://evil.example/login").collect();
        assert_eq!(hits.len(), 2, "{skin:?}: the raw line and the read one");
        for (i, _) in hits {
            let sgr = format!("\x1b[{}", out[..i].rsplit("\x1b[").next().unwrap());
            assert_eq!(sgr, block(), "{skin:?}: the target of a link that lies");
        }
        // what is honest stays as UI-8 draws it: not the alert's
        for fine in ["https://fine.example/docs", "https://fine.example/x"] {
            assert_ne!(seq(&out, fine), block(), "{skin:?}: {fine}");
        }
    }
}

#[test]
fn md_10_the_read_view_hides_no_char_of_such_a_link() {
    let r = shown("md10-whole", Skin::Depth, false, LINKS);
    let rows = &r.shot.frame.rows;
    let line = "[https://good.example/docs](https://evil.example/login) read";
    assert!(rows.iter().any(|x| x.text.contains(line)), "{:#?}", r.editor());
    assert!(
        rows.iter().any(|x| x
            .text
            .contains("[https://good.example/docs](https://evil.example/login) raw"))
    );
}

#[test]
fn md_10_without_color_the_target_is_inverse_and_bold() {
    let r = shown("md10-mono", Skin::Depth, true, LINKS);
    let out = draw(r.shot.clone());
    let sgr = seq(&out, "https://evil.example/login");
    assert!(sgr.contains(";7") && sgr.contains(";1"), "inverse and bold: {sgr:?}");
    let fine = seq(&out, "https://fine.example/docs");
    assert!(!fine.contains(";7"), "an honest target is not: {fine:?}");
    assert!(!out.contains("38;") && !out.contains("48;"), "and no color at all: {out:?}");
}

#[test]
fn md_10_calm_draws_the_link_as_it_did_and_the_target_is_on_the_screen() {
    let r = shown("md10-calm", Skin::Calm, false, LINKS);
    let out = draw(r.shot.clone());
    assert!(out.contains("https://evil.example/login"), "{out:?}");
    assert_ne!(seq(&out, "https://evil.example/login"), block(), "calm has no interface layer");
}

#[test]
fn md_10_a_link_that_says_what_it_is_is_not_marked() {
    let honest = "[docs](https://one.example/docs) [two.example](https://two.example) x\n";
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md10-honest", skin, false, honest);
        let out = draw(r.shot.clone());
        for what in ["https://one.example/docs", "https://two.example"] {
            assert_ne!(seq(&out, what), block(), "{skin:?}: {what}");
        }
    }
}

#[test]
fn md_10_a_char_shown_in_another_form_in_such_a_target_is_still_shown_as_itself() {
    let line = "[https://good.example](https://evil\u{202e}.example) end\n";
    for skin in [Skin::Flat, Skin::Depth] {
        let r = shown("md10-hostile", skin, false, line);
        let rows: String = r.shot.frame.rows.iter().map(|x| x.text.clone()).collect();
        assert!(rows.contains("<U+202E>") && !rows.contains('\u{202e}'), "{skin:?}: {rows:?}");
    }
}
