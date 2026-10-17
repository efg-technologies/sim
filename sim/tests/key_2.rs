//! KEY-2, KEY-5: Space is a leader in Normal mode and in the tree, the keys of its sequences are
//! sim's, and `<Space>bp` and `<Space>bc` put letters on the tabs, as bufferline's pick does.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim::tabs::letters;
use sim_core::core::app::Event;

const FILES: [&str; 3] = ["point.rs=fn a() {}\n", "lib.rs=fn b() {}\n", "README.md=# t\n"];

/// Three files open (point.rs shown last opened is README.md), in the interface.
fn open3(name: &str, skin: Skin) -> Rig {
    let mut r = rig_env(name, &FILES, &["point.rs"], (100, 24), |e| {
        e.skin = skin;
        e.chrome = true;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r.keys(":e lib.rs\n:e README.md\n");
    assert_eq!(r.open_files(), ["point.rs", "lib.rs", "README.md"]);
    r
}

/// The tab bar's row.
fn bar(r: &Rig) -> String {
    r.screen.rows[usize::from(r.sim.geo().y0) - 1].text.clone()
}

#[test]
fn key_2_space_starts_a_sequence_in_normal_mode_and_in_the_tree() {
    let mut r = open3("k2-lead", Skin::Depth);
    let text = r.text();
    r.keys(" ");
    assert_eq!(r.sim.prefix(), ["<Space>"]);
    assert_eq!(r.text(), text, "the core was given nothing");
    r.keys("b");
    assert_eq!(r.sim.prefix(), ["<Space>", "b"]);
    r.keys("<Esc>");
    assert!(r.sim.prefix().is_empty(), "Esc ends it");
    // in the tree
    r.keys("<C-n> ");
    assert!(r.sim.panel.focus && r.sim.prefix() == ["<Space>"], "{:?}", r.sim.prefix());
    r.keys("bp");
    assert!(r.sim.prefix().is_empty(), "the letters are on the tabs now");
    r.keys("<Esc>");
}

#[test]
fn key_2_a_key_no_sequence_has_does_nothing_and_is_not_the_core_s() {
    let mut r = open3("k2-none", Skin::Depth);
    let (text, cur) = (r.text(), r.sim.cur);
    // `x` would delete a char, `w` move: after a Space they are only the end of it
    for keys in [" x", " w", " bx", " bq", " <Tab>", " <C-n>", " :"] {
        r.keys(keys);
        assert!(r.sim.prefix().is_empty(), "{keys:?}");
        assert_eq!((r.text(), r.sim.cur), (text.clone(), cur), "{keys:?} did nothing");
        assert!(!r.sim.panel.focus && !r.sim.picker_open(), "{keys:?}");
        assert_eq!(r.app().vi.mode, sim_core::core::vi::Mode::Normal, "{keys:?}");
    }
    // and the key after the end is a key again
    r.keys(" xx");
    assert_eq!(
        r.text(),
        text[1..],
        "the first x was the end of the Space, the second deleted a char"
    );
}

#[test]
fn key_2_space_is_text_where_text_is_typed_and_not_a_leader_after_an_operator_or_in_visual_mode() {
    let mut r = open3("k2-text", Skin::Depth);
    let text = r.text();
    r.keys("i <Esc>"); // Insert mode
    assert_eq!(r.text(), format!(" {text}"));
    assert!(r.sim.prefix().is_empty());
    r.keys(": ");
    assert_eq!(r.app().vi.line, ": ", "the command line has it");
    r.keys("<Esc>");
    r.keys("v ");
    assert!(r.sim.prefix().is_empty(), "Visual mode: no leader");
    r.keys("<Esc>d ");
    assert!(r.sim.prefix().is_empty(), "after an operator: the operator ends");
    // the tree's query and the picker's
    r.keys("<C-n>/ ");
    assert_eq!(r.sim.prefix(), Vec::<String>::new());
    r.keys("<Esc><Esc><C-p> a");
    assert_eq!(r.query(), " a", "the picker's query");
}

#[test]
fn key_5_the_letters_are_bufferline_s() {
    let s = |v: &[&str]| v.iter().map(ToString::to_string).collect::<Vec<_>>();
    assert_eq!(letters(&s(&["point.rs", "lib.rs", "README.md"])), ['p', 'l', 'R']);
    // a letter that is taken: the first free one of the alphabet
    assert_eq!(letters(&s(&["a.rs", "a.md", "a.txt"])), ['a', 'b', 'c']);
    // no `n` in the alphabet (it is `abcdefghijklmopqrstuvwxyz`), a dot and a Japanese name are not in it either
    assert_eq!(letters(&s(&["notes.md", ".gitignore", "日本語.txt"])), ['a', 'b', 'c']);
    assert_eq!(letters(&s(&["1.txt", "Zeta.rs", "x.rs"])), ['1', 'Z', 'x']);
    // the alphabet's end, and tabs beyond it have no letter
    let many: Vec<String> = (0..70).map(|i| format!(".f{i}")).collect();
    let l = letters(&many);
    assert_eq!(l[0..3], ['a', 'b', 'c']);
    assert_eq!(l[59], '0');
    assert_eq!(l[60..], [' '; 10]);
}

#[test]
fn key_5_space_b_p_puts_a_letter_on_each_tab_and_the_letter_shows_its_file() {
    let mut r = open3("k5-pick", Skin::Depth);
    assert_eq!(r.sim.cur, 2);
    assert!(!bar(&r).contains(" p  point.rs"), "{}", bar(&r));
    r.keys(" bp");
    let row = bar(&r);
    assert!(
        row.contains(" p  point.rs") && row.contains(" l  lib.rs") && row.contains(" R  README.md"),
        "{row}"
    );
    r.keys("p");
    assert_eq!((r.sim.cur, r.app().path.as_deref()), (0, Some("point.rs")));
    assert!(!bar(&r).contains(" p  point.rs"), "the letters are gone: {}", bar(&r));
    // Esc cancels, and any other key does and does nothing
    r.keys(" bp<Esc>");
    assert_eq!((r.sim.cur, bar(&r).contains(" l  lib.rs")), (0, false));
    r.keys(" bpz");
    assert!(
        r.sim.prefix().is_empty(),
        "the z was the end of it, not the start of `zz`: {:?}",
        r.sim.prefix()
    );
    // a letter no tab has
    r.keys(" bpq");
    assert_eq!(r.sim.cur, 0);
    // from the tree the letter shows the file and gives the text the focus
    r.keys("<C-n> bpR");
    assert_eq!((r.sim.cur, r.sim.panel.focus), (2, false));
}

#[test]
fn key_5_without_a_tab_bar_the_letters_are_on_the_status_line() {
    let mut r = open3("k5-calm", Skin::Calm);
    r.keys(" bp");
    assert!(r.status().contains("pick: p point.rs · l lib.rs · R README.md"), "{}", r.status());
    r.keys("l");
    assert_eq!(r.app().path.as_deref(), Some("lib.rs"));
    r.keys(" bc");
    assert!(r.status().contains("pick to close: "), "{}", r.status());
}

#[test]
fn key_5_space_b_c_closes_a_file_as_q_does_and_never_one_with_changes() {
    let mut r = open3("k5-close", Skin::Depth);
    r.keys(" bcl"); // lib.rs, clean, not the one shown
    assert_eq!(r.open_files(), ["point.rs", "README.md"]);
    assert_eq!(r.app().path.as_deref(), Some("README.md"), "the one shown stays");
    // a file with changes is not closed: it is shown, and says why
    r.keys("x"); // README.md changed
    r.keys("<Tab> bcR");
    assert_eq!(r.open_files(), ["point.rs", "README.md"], "still open");
    assert_eq!(r.app().path.as_deref(), Some("README.md"), "and shown");
    assert!(r.status().contains("no write since last change"), "{}", r.status());
    assert!(r.app().buf.dirty(), "nothing was lost");
    // the last file: it quits, as :q does
    r.keys(":w\n");
    r.keys(" bcp");
    assert_eq!(r.open_files(), ["README.md"]);
    r.keys(" bcR");
    assert_eq!(r.quit, Some(0));
}
