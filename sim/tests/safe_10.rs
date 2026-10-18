//! SAFE-10, SAFE-11 in the editor: wherever sim draws a char of a file or of a name, a char that is
//! invisible is its form (the core's `show` decides it, and sim asks it: one table). The surfaces
//! other than the text: the tab bar, the breadcrumb, the status line, the branch, the messages that
//! quote a register or a mark, the read view of Markdown, in every look, and the bytes the painter
//! writes for them.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::layout::Skin;
use sim::paint::Painter;
use sim::sim::SimEvent;
use sim_core::core::app::Event;

/// Everything the screen holds as text, and the bytes the painter writes for it with its escape
/// sequences taken away.
fn written(r: &Rig) -> (String, String) {
    let rows = r.shot.frame.rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join("\n");
    let mut p = Painter::new(Vec::new());
    p.draw(r.shot.clone()).unwrap();
    let out = String::from_utf8(p.out).unwrap();
    let (mut text, mut cs) = (String::new(), out.chars());
    while let Some(c) = cs.next() {
        if c == '\x1b' {
            assert_eq!(cs.next(), Some('['), "only CSI sequences are written");
            for c in cs.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        } else {
            text.push(c);
        }
    }
    (rows, text)
}

/// The screen, and what was written for it, holds no char that SAFE-10 hides but the legitimate uses.
fn assert_safe(r: &Rig, what: &str) {
    let mut p = Painter::new(Vec::new());
    p.draw(r.shot.clone()).unwrap();
    assert_eq!(foreign(&p.out), Vec::<String>::new(), "{what}");
    let (rows, bytes) = written(r);
    for (kind, text) in [("rows", rows), ("bytes", bytes)] {
        assert_eq!(oracle::raw_hidden(&text), None, "{what}: {kind}: {text:?}");
    }
}

/// The screen holds `form` (somewhere), in the rows and in the bytes.
fn assert_shows(r: &Rig, what: &str, form: &str) {
    let (rows, bytes) = written(r);
    assert!(rows.contains(form), "{what}: {form:?} is not in the rows {rows:?}");
    assert!(bytes.contains(form), "{what}: {form:?} is not in the bytes {bytes:?}");
}

const LOOKS: [Skin; 3] = [Skin::Calm, Skin::Flat, Skin::Depth];

/// A rig in `skin`, with the interface (the tab bar, the bar, the breadcrumb), started on `open`.
fn looked(name: &str, skin: Skin, files: &[&str], open: &[&str], size: (u16, u16)) -> Rig {
    let mut r = rig_env(name, files, open, size, |e| {
        e.skin = skin;
        e.chrome = true;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r.hold_spans = true;
    r.keys("l"); // (the first key draws the interface, which waits for the terminal's colors)
    r.deliver_spans();
    r
}

const NAME: &str = "e\u{e0041}\u{3164}\u{fe0f}x.md";
const FORM: &str = "e<U+E0041><U+3164><U+FE0F>x.md";

#[test]
fn safe_10_a_name_with_hidden_characters_is_its_form_in_the_tab_bar_the_crumb_and_the_status_line()
{
    for skin in LOOKS {
        let content = "# Head\u{e0049}\u{200b}\n\ntext\n";
        let mut r = looked("s10-name", skin, &[&format!("{NAME}={content}")], &[NAME], (100, 16));
        assert_shows(&r, &format!("{skin:?}"), FORM);
        assert_safe(&r, &format!("{skin:?}"));
        r.keys("j");
        assert_safe(&r, &format!("{skin:?}, the cursor on the heading"));
        // flat and depth name the file in the tab bar or the title, and the section in the crumb
        if skin != Skin::Calm {
            assert!(
                r.shot.frame.rows[..3].iter().any(|x| x.text.contains(FORM)),
                "{skin:?}: {:?}",
                &r.shot.frame.rows[..3]
            );
        }
    }
}

#[test]
fn safe_10_a_branch_with_hidden_characters_is_its_form_in_the_bar() {
    // (the branch is the first entry of git's status; calm has no bar for it, flat and depth have)
    let status = "## feat/\u{e0049}\u{e0047}x\u{200b}...origin/feat\0".as_bytes().to_vec();
    for skin in LOOKS {
        let mut r = looked("s10-branch", skin, &["a.rs"], &["a.rs"], (100, 16));
        r.send(SimEvent::Status(Ok((status.clone(), String::new(), vec![]))));
        assert_safe(&r, &format!("{skin:?}"));
        if skin != Skin::Calm {
            assert_shows(&r, &format!("{skin:?}"), "feat/<U+E0049><U+E0047>x<U+200B>");
        }
    }
}

#[test]
fn safe_10_a_register_and_a_mark_say_their_text_in_its_forms() {
    let mut r = rig("s10-reg", &["f.txt=a\u{e0049}\u{e0047}b\u{200b}c\u{3164}\n"], &["f.txt"]);
    r.keys("yy:reg\n");
    assert!(r.status().contains("a<U+E0049><U+E0047>b<U+200B>c<U+3164>"), "{}", r.status());
    r.keys("ma:marks\n");
    assert!(r.status().starts_with("'a 1:1 a<U+E0049>"), "{}", r.status());
    assert_safe(&r, "the message");
}

#[test]
fn safe_10_the_read_view_of_markdown_draws_hidden_characters_as_forms() {
    let md = "# Head\u{e0049}\u{200b}\n\nA [link\u{e0049}](http://x.y) and **bold\u{3164}** text\n\n> quote\u{2028}here\n\n- item\u{ad}one\n";
    for look in LOOKS {
        let mut r = looked("s10-md", look, &[&format!("a.md={md}")], &["a.md"], (100, 20));
        r.keys("<Space>um");
        assert_safe(&r, &format!("{look:?}"));
        for form in [
            "Head<U+E0049><U+200B>",
            "link<U+E0049>",
            "bold<U+3164>",
            "quote<U+2028>here",
            "item<U+00AD>one",
        ] {
            assert_shows(&r, &format!("{look:?}"), form);
        }
        r.keys("jjjj");
        assert_safe(&r, &format!("{look:?}, the cursor in the text"));
    }
}

#[test]
fn safe_10_the_command_line_and_the_search_show_what_is_typed() {
    let mut r = rig("s10-cmd", &["f.txt=x\n"], &["f.txt"]);
    r.send(SimEvent::Term(Event::Key(sim_core::core::app::Key::Char(':'))));
    r.send(SimEvent::Term(Event::Paste("a\u{e0041}b\u{2060}".into())));
    assert_shows(&r, "the command line", ":a<U+E0041>b<U+2060>");
    assert_safe(&r, "the command line");
    r.keys("<Esc>/q\u{200b}\u{e0041}q\n");
    assert_shows(&r, "a failed search", "q<U+200B><U+E0041>q");
    assert_safe(&r, "a failed search");
}

#[test]
fn safe_11_what_is_drawn_as_it_is_in_a_file_stays_so_in_every_look() {
    let text = "👨\u{200d}👩 ⚠\u{fe0f} 🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f} 1\u{fe0f}\u{20e3}\n";
    for skin in LOOKS {
        let r = looked("s11-legit", skin, &[&format!("a.txt={text}")], &["a.txt"], (60, 8));
        let (rows, bytes) = written(&r);
        for part in [
            "👨\u{200d}👩",
            "⚠\u{fe0f}",
            "\u{1f3f4}\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}",
            "1\u{fe0f}\u{20e3}",
        ] {
            assert!(rows.contains(part) && bytes.contains(part), "{skin:?}: {part:?} in {rows:?}");
        }
        assert!(!rows.contains("<U+"), "nothing legitimate is shown as a form: {rows:?}");
    }
}
