//! JUMP-1: `s` jumps by label, as the owner's flash.nvim does: a literal smart-case query, every match on the
//! screen lit and labelled after it, a label moves the cursor there, Enter to the nearest, Esc nowhere; it
//! never edits, and plain mode has none.
#![allow(clippy::unwrap_used, clippy::expect_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::{Syntax, Tones};
use sim::layout::Skin;
use sim::paint::{Cue, Look};
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use sim_core::core::color::palette;
use sim_core::core::vi::Mode;

const TEXT: &str = "foo bar\nfox baz\nfob\n";

/// The cursor as `line:col`, 1-based.
fn at(r: &Rig) -> (usize, usize) {
    let b = &r.app().buf;
    (b.line_of(b.cursor) + 1, b.col() + 1)
}

/// The text of the editor's rows, after the gutter of the line numbers (`   1 `).
fn lines(r: &Rig) -> Vec<String> {
    r.editor().iter().map(|l| l.chars().skip(5).collect::<String>()).collect()
}

fn depth(name: &str, files: &[&str], open: &[&str]) -> Rig {
    let mut r = rig_env(name, files, open, (80, 24), |e| {
        e.skin = Skin::Depth;
        e.chrome = true;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r
}

#[test]
fn jump_1_s_and_a_label_move_the_cursor_to_the_match_and_nothing_is_edited() {
    let mut r = rig("j1-label", &[&format!("a.txt={TEXT}")], &["a.txt"]);
    r.keys("sfo");
    assert_eq!(r.status(), "jump: fo");
    // the char after each match carries the label; `o`, `x` and `b` follow a match, so they are no labels,
    // and the nearest match (the cursor's own) has the first one
    assert_eq!(lines(&r)[..3], ["foa bar", "fos baz", "fod"]);
    r.keys("d");
    assert_eq!(at(&r), (3, 1));
    assert_eq!(r.text(), TEXT.trim_end());
    assert!(!r.app().buf.dirty());
    assert_eq!(lines(&r)[..3], ["foo bar", "fox baz", "fob"], "the labels are gone");
    assert!(!r.status().starts_with("jump"), "{}", r.status());
    // a jump is VI-17's: `''` goes back
    r.keys("''");
    assert_eq!(at(&r), (1, 1));
}

#[test]
fn jump_1_a_letter_that_could_go_on_with_the_query_is_never_a_label() {
    let mut r = rig("j1-skip", &["a.txt=xa xs xd xA\n"], &["a.txt"]);
    r.keys("sx");
    // `a`, `s`, `d` (and `A`) follow a match: the labels are `f`, `g`, `h`, `j`, nearest first
    assert_eq!(lines(&r)[0], "xf xg xh xj");
    // so `s` is more of the query, and the one match of `xs` has the first label
    r.keys("s");
    assert_eq!(r.status(), "jump: xs");
    assert_eq!(lines(&r)[0], "xa xsaxd xA");
    r.keys("a");
    assert_eq!(at(&r), (1, 4));
}

#[test]
fn jump_1_smart_case_one_capital_makes_the_query_exact() {
    let mut r = rig("j1-case", &["a.txt=Foo foo FOO\n"], &["a.txt"]);
    r.keys("sfo");
    assert_eq!(lines(&r)[0], "Foa fos FOd", "lower case matches any case");
    r.keys("<Esc>sFo");
    assert_eq!(lines(&r)[0], "Foa foo FOO", "a capital makes it exact");
}

#[test]
fn jump_1_enter_goes_to_the_nearest_match_esc_nowhere_and_backspace_takes_a_char_off() {
    let mut r = rig("j1-keys", &["a.txt=one\ntwo\nthree two\none\n"], &["a.txt"]);
    r.keys("jj$stw");
    assert_eq!(lines(&r)[..4], ["one", "tws", "three twa", "one"]);
    r.keys("<BS>");
    assert_eq!(r.status(), "jump: t");
    r.keys("<Esc>");
    assert_eq!(at(&r), (3, 9), "Esc: nothing moved");
    assert!(!r.status().starts_with("jump"));
    r.keys("stwo\n");
    assert_eq!(at(&r), (3, 7), "Enter: the nearest match");
    // a char that leaves no match ends the jump, with nothing moved
    r.keys("sq");
    assert!(!r.status().starts_with("jump"), "{}", r.status());
    assert_eq!(at(&r), (3, 7));
    assert_eq!(r.text(), "one\ntwo\nthree two\none");
}

#[test]
fn jump_1_in_visual_mode_the_selection_goes_with_the_cursor() {
    let mut r = rig("j1-visual", &[&format!("a.txt={TEXT}")], &["a.txt"]);
    r.keys("vsfo");
    assert_eq!(r.status(), "jump: fo");
    r.keys("s");
    assert_eq!(r.app().vi.mode, Mode::Visual);
    assert_eq!(at(&r), (2, 1));
    r.keys("d");
    assert_eq!(
        r.text(),
        "ox baz\nfob",
        "the selection went from where it began to the label's match"
    );
}

#[test]
fn jump_1_every_other_key_ends_it_and_does_nothing_else() {
    let mut r = rig("j1-end", &[&format!("a.txt={TEXT}"), "b.txt=other\n"], &["a.txt"]);
    r.keys(":e b.txt\n<Tab>");
    assert_eq!(r.open_files()[r.sim.cur], "a.txt");
    for k in ["<Tab>", "<C-p>", "<C-c>", "<Down>", "<F1>", "<S-Tab>"] {
        r.keys("sf").keys(k);
        assert!(!r.status().starts_with("jump"), "{k}: {}", r.status());
        assert_eq!(r.open_files()[r.sim.cur], "a.txt", "{k}");
        assert!(!r.sim.picker_open(), "{k}");
        assert_eq!(at(&r), (1, 1), "{k}");
    }
    // a paste is not put in
    r.keys("sf");
    r.send(SimEvent::Term(Event::Paste("zzz".into())));
    assert_eq!(r.text(), TEXT.trim_end());
    assert!(!r.status().starts_with("jump"));
    // the mouse ends it, and then does what it does
    r.keys("sfo");
    r.click(5, 1);
    assert!(!r.status().starts_with("jump"));
    assert_eq!(lines(&r)[1], "fox baz");
    assert_eq!(at(&r), (2, 1));
}

#[test]
fn jump_1_plain_mode_has_none_and_s_does_nothing_there() {
    let mut r = rig("j1-plain", &[&format!("a.txt={TEXT}")], &["--plain", "a.txt"]);
    r.keys("s");
    assert!(!r.status().starts_with("jump"), "{}", r.status());
    assert_eq!(r.text(), TEXT.trim_end());
    assert_eq!(at(&r), (1, 1));
}

#[test]
fn jump_1_a_label_is_never_on_a_form_and_is_fullwidth_over_a_wide_char() {
    let mut r = rig("j1-forms", &["a.txt=ab\u{202e}c abX\n日本語 日本\n"], &["a.txt"]);
    r.keys("sab");
    // the first match is followed by U+202E, drawn as its form: it gets no label, and the form is whole
    assert_eq!(lines(&r)[0], "ab<U+202E>c aba");
    r.keys("<Esc>s日本");
    // the label over a wide char is its fullwidth form; at the end of the line it is past it
    assert_eq!(lines(&r)[1], "日本ａ 日本s");
    r.keys("s");
    assert_eq!(at(&r), (2, 5));
    assert_eq!(r.text(), "ab\u{202e}c abX\n日本語 日本");
}

#[test]
fn jump_1_in_the_read_view_every_line_is_raw_while_a_jump_is_typed() {
    let mut r = rig_env("j1-md", &["a.md=x\n**foo** and `fox`\n"], &["a.md"], (80, 24), |e| {
        e.md = true;
    });
    assert!(lines(&r)[1].starts_with("foo and fox"), "{:?}", lines(&r));
    r.keys("sfo");
    assert_eq!(lines(&r)[1], "**foa** and `fos`");
    r.keys("<Esc>");
    assert!(lines(&r)[1].starts_with("foo and fox"), "{:?}", lines(&r));
}

#[test]
fn jump_1_the_matches_are_the_accent_s_block_and_the_labels_bold_on_its_solid_color() {
    let mut r = depth("j1-looks", &[&format!("a.txt={TEXT}")], &["a.txt"]);
    let t = Tones::with(&palette((0x1e, 0x1e, 0x1e)), &Syntax::new(true));
    let (block, _) = t.accent_block();
    let (solid, _) = t.accent_solid();
    r.keys("sfo");
    let row = r.shot.frame.rows.iter().position(|w| w.text.contains("foa bar")).unwrap();
    let x = r.shot.frame.rows[row].text.chars().position(|c| c == 'a').unwrap() as u16;
    let first =
        |col: u16| r.shot.paints[row].iter().find(|(c, p)| c.contains(&col) && p.bg.is_some());
    let label = first(x).unwrap().1;
    assert_eq!((label.bg, label.bold), (Some(solid), true));
    assert_eq!(first(x - 1).unwrap().1.bg, Some(block), "the match");
    // calm: no match is lit, and the label is on the selection's gray
    let mut c = rig_env("j1-calm", &[&format!("a.txt={TEXT}")], &["a.txt"], (80, 24), |e| {
        e.syn = Syntax::new(true);
    });
    c.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    c.keys("sfo");
    let row = c.shot.frame.rows.iter().position(|w| w.text.contains("foa bar")).unwrap();
    let x = c.shot.frame.rows[row].text.chars().position(|ch| ch == 'a').unwrap() as u16;
    assert!(c.shot.looks[row].iter().any(|(r, l)| r.contains(&x) && *l == Look::Sel));
    assert!(!c.shot.looks[row].iter().any(|(r, l)| r.contains(&(x - 1)) && *l == Look::Sel));
    // no color: the label is inverse
    let mut m = rig_env("j1-mono", &[&format!("a.txt={TEXT}")], &["a.txt"], (80, 24), |e| {
        e.skin = Skin::Depth;
        e.chrome = true;
        e.syn = Syntax::mono();
    });
    m.keys("sfo");
    let row = m.shot.frame.rows.iter().position(|w| w.text.contains("foa bar")).unwrap();
    let x = m.shot.frame.rows[row].text.chars().position(|ch| ch == 'a').unwrap() as u16;
    let cue = m.shot.paints[row].iter().find(|(c, p)| c.contains(&x) && p.cue != Cue::None);
    assert_eq!(cue.unwrap().1.cue, Cue::Solid);
}

#[test]
fn jump_1_a_change_from_the_disk_while_a_jump_is_typed_moves_the_labels_with_the_text() {
    let mut r = rig("j1-disk", &[&format!("a.txt={TEXT}")], &["a.txt"]);
    r.keys("sfo");
    let id = r.sim.bufs[r.sim.cur].id;
    r.send(SimEvent::App(id, disk(b"new\nfoo bar\nfox baz\nfob\n")));
    assert_eq!(r.status(), "jump: fo", "the jump goes on");
    assert_eq!(lines(&r)[..4], ["new", "foa bar", "fos baz", "fod"]);
    r.keys("s");
    assert_eq!(at(&r), (3, 1), "the label's match, where the text has it now");
}
