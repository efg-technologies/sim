//! KEY-7: the owner's leader keys of Telescope and Diffview: `<Space>ff` the finder, `<Space>fg` the search of
//! contents, `<Space>fk` the help, `<Space>ft` the search of contents with `TODO` typed, `<Space>gg` the changes
//! view; and which-key lists them.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::color::Syntax;
use sim::keys::next;
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim_core::core::app::Event;

const FILES: [&str; 3] =
    ["a.rs=fn a() {} // TODO: one\n", "b.rs=// todo: lower case\n", "c.rs=fn c() {}\n"];

fn rig7(name: &str) -> Rig {
    let mut r = rig_env(name, &FILES, &["a.rs"], (100, 30), |e| {
        e.skin = Skin::Depth;
        e.chrome = true;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r
}

/// The title in the picker's top border.
fn title(r: &Rig) -> String {
    r.float().first().cloned().unwrap_or_default()
}

#[test]
fn key_7_space_f_f_g_and_k_open_the_finder_the_search_of_contents_and_the_help() {
    let mut r = rig7("k7-open");
    r.keys(" ff");
    assert!(r.sim.picker_open() && title(&r).contains("Find files"), "{}", title(&r));
    assert_eq!(r.query(), "");
    assert_eq!(r.text(), "fn a() {} // TODO: one", "the text was not typed into");
    r.keys("<Esc> fg");
    assert!(r.sim.picker_open() && title(&r).contains("Live grep"), "{}", title(&r));
    assert_eq!(r.query(), "");
    r.keys("<Esc> fk");
    assert!(r.sim.picker_open() && title(&r).contains("Help · keys and commands"), "{}", title(&r));
    r.keys("<Esc>");
    assert!(!r.sim.picker_open());
    // from the tree too, where Space is the leader
    r.keys("<C-n> ff");
    assert!(r.sim.picker_open() && title(&r).contains("Find files"), "{}", title(&r));
}

#[test]
fn key_7_space_f_t_searches_the_contents_for_todo_exactly_and_the_query_can_be_edited() {
    let mut r = rig7("k7-todo");
    r.keys(" ft");
    assert!(title(&r).contains("Live grep"), "{}", title(&r));
    assert_eq!(r.query(), "TODO");
    let list = r.list();
    assert!(list.iter().any(|l| l.contains("a.rs:1")), "{list:?}");
    // a literal with a capital is exact: the lower-case one is not found
    assert!(!list.iter().any(|l| l.contains("b.rs")), "{list:?}");
    r.keys("<BS><BS><BS><BS>todo");
    assert_eq!(r.query(), "todo");
    let list = r.list();
    assert!(list.iter().any(|l| l.contains("b.rs:1")), "{list:?}");
}

#[test]
fn key_7_space_g_g_is_the_changes_view_as_space_g_e_and_again_goes_back_to_the_tree() {
    let mut r = rig_env("k7-gg", &CHANGED_FILES, &[], (100, 24), |e| {
        changed_repo(&e.cwd);
    });
    r.keys(" gg");
    assert!(r.sim.panel.open && r.sim.panel.focus);
    assert!(r.status().ends_with("  CHANGES"), "{}", r.status());
    let view = r.panel();
    r.keys(" gg");
    assert!(r.status().ends_with("  TREE"), "{}", r.status());
    r.keys(" ge");
    assert_eq!(r.panel(), view, "the same view as <Space>ge's");
}

#[test]
fn key_7_space_g_g_in_plain_mode_is_refused_as_space_g_e_is() {
    let mut r = rig("k7-plain", &FILES, &["--plain", "a.rs"]);
    r.keys(" gg");
    assert_eq!(r.status(), "plain mode runs no program: no changes view");
    assert!(!r.sim.panel.open);
}

#[test]
fn key_7_which_key_lists_the_keys_after_space_f_and_space_g_from_the_table() {
    let p = |s: &[&str]| s.iter().map(ToString::to_string).collect::<Vec<_>>();
    let f: Vec<String> = next(&p(&["<Space>", "f"]), 'n').into_iter().map(|r| r.0).collect();
    assert_eq!(f, ["f", "g", "k", "t"]);
    let g: Vec<String> = next(&p(&["<Space>", "g"]), 't').into_iter().map(|r| r.0).collect();
    assert!(g.contains(&"g".to_string()) && g.contains(&"e".to_string()), "{g:?}");
    let space: Vec<String> = next(&p(&["<Space>"]), 'n').into_iter().map(|r| r.0).collect();
    assert!(space.contains(&"f".to_string()), "{space:?}");
    // and the window shows them after a pause
    let mut r = rig7("k7-which");
    r.keys(" f").pause();
    let shot = dump(&r.screen);
    assert!(shot.contains("find files") && shot.contains("search for TODO"), "{shot}");
}
