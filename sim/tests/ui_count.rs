//! UI-17: while the matches of a search are lit, the status line says which one the cursor is at and
//! how many there are, `[3/12]`, as Neovim's searchcount does: `[0/12]` before the first match,
//! `[5/>99]` with more than 99 and `[>99/>99]` from the hundredth on.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim_core::core::app::Event;

const FOO: &str = "foo bar foo\nbaz Foo\nqux foo\n日本語 foo\n";

fn rig_count(name: &str, skin: Skin, files: &[&str]) -> Rig {
    let args = ["a.txt"];
    let mut r = rig_env(name, files, &args, (100, 20), |e| {
        e.skin = skin;
        e.syn = Syntax::new(true);
        e.chrome = true;
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r
}

/// The `[i/n]` the status line says, if it does.
fn count(r: &Rig) -> Option<String> {
    let s = r.status();
    let at = s.find('[')?;
    let end = s[at..].find(']')?;
    Some(s[at..=at + end].to_string()).filter(|c| c.contains('/'))
}

#[test]
fn ui_17_the_status_line_says_which_match_the_cursor_is_at_and_how_many_there_are() {
    for skin in [Skin::Calm, Skin::Flat, Skin::Depth] {
        let mut r = rig_count("count-basic", skin, &[&format!("a.txt={FOO}")]);
        assert_eq!(count(&r), None, "{skin:?}: no search, no count");
        r.keys("/foo\n"); // the first match after the cursor: the second `foo` of the first line
        assert_eq!(count(&r).as_deref(), Some("[2/5]"), "{skin:?}: {}", r.status());
        r.keys("n");
        assert_eq!(count(&r).as_deref(), Some("[3/5]"), "{skin:?}");
        r.keys("nnn");
        assert_eq!(count(&r).as_deref(), Some("[1/5]"), "{skin:?}: it wraps");
        r.keys("N");
        assert_eq!(count(&r).as_deref(), Some("[5/5]"), "{skin:?}");
        r.keys("gg");
        assert_eq!(
            count(&r).as_deref(),
            Some("[1/5]"),
            "{skin:?}: the cursor is on the first match"
        );
        // the cursor on `bar` is after the first match and before the second
        r.keys("llll");
        assert_eq!(count(&r).as_deref(), Some("[1/5]"), "{skin:?}");
        r.keys("w");
        assert_eq!(count(&r).as_deref(), Some("[2/5]"), "{skin:?}: the second `foo` starts there");
    }
}

#[test]
fn ui_17_before_the_first_match_it_is_zero_and_the_count_goes_with_the_highlight() {
    let mut r = rig_count("count-zero", Skin::Depth, &["a.txt=x foo\nfoo y\n"]);
    r.keys("/foo\n");
    assert_eq!(count(&r).as_deref(), Some("[1/2]"));
    r.keys("gg0");
    assert_eq!(count(&r).as_deref(), Some("[0/2]"), "{}", r.status());
    // Esc puts the matches out, and the count with them; `n` lights them and says it again
    r.keys("<Esc>");
    assert_eq!(count(&r), None, "{}", r.status());
    r.keys("n");
    assert_eq!(count(&r).as_deref(), Some("[1/2]"));
    r.keys(":noh\n");
    assert_eq!(count(&r), None);
    // no match: nothing to count (the core says so)
    r.keys("/zzz\n");
    assert_eq!(count(&r), None, "{}", r.status());
}

#[test]
fn ui_17_past_99_matches_it_says_so_as_neovim_does() {
    let text: String = (0..150).map(|i| format!("foo {i}\n")).collect();
    let mut r = rig_count("count-many", Skin::Depth, &[&format!("a.txt={text}")]);
    r.keys("/foo\ngg");
    assert_eq!(count(&r).as_deref(), Some("[1/>99]"), "{}", r.status());
    r.keys("99G");
    assert_eq!(count(&r).as_deref(), Some("[99/>99]"));
    r.keys("100G");
    assert_eq!(count(&r).as_deref(), Some("[>99/>99]"));
    r.keys("150G");
    assert_eq!(count(&r).as_deref(), Some("[>99/>99]"));
}

#[test]
fn ui_17_the_count_follows_an_edit_and_a_long_file_says_nothing() {
    let mut r = rig_count("count-edit", Skin::Depth, &[&format!("a.txt={FOO}")]);
    r.keys("/foo\ngg");
    assert_eq!(count(&r).as_deref(), Some("[1/5]"));
    r.keys("jdd"); // a line with one match goes, and the cursor is on the next line, after two matches
    assert_eq!(count(&r).as_deref(), Some("[2/4]"), "{}", r.status());
    // a file of more than HIT_LINES lines has no count (a scan on the way of every edit)
    let long: String = (0..6000).map(|i| format!("foo {i}\n")).collect();
    let mut r = rig_count("count-long", Skin::Depth, &[&format!("a.txt={long}")]);
    r.keys("/foo\n");
    assert_eq!(count(&r), None, "{}", r.status());
}
