//! GUI-5: `Sim::wants_text` says whether the next key is text (typed in Insert mode, on the command
//! line, in the picker's query, in the tree's search, the char after `f` or `r`) and not a command: a
//! window turns its input method on for it, and off where a letter is a command.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;

fn rig_with_tree() -> Rig {
    rig("g5-text", &["a.txt=hello world\nsecond\n", "b.txt=2\n"], &["a.txt"])
}

#[test]
fn gui_5_a_letter_is_a_command_in_normal_mode_and_text_in_insert_mode() {
    let mut r = rig_with_tree();
    assert!(!r.sim.wants_text(), "normal mode");
    r.keys("i");
    assert!(r.sim.wants_text(), "insert mode");
    r.keys("x<Esc>");
    assert!(!r.sim.wants_text(), "and back");
}

#[test]
fn gui_5_the_char_after_f_t_or_r_is_text_and_the_operators_before_a_motion_are_not() {
    let mut r = rig_with_tree();
    for pre in ["f", "F", "t", "T", "r"] {
        r.keys(pre);
        assert!(r.sim.wants_text(), "after {pre}");
        r.keys("<Esc>");
        assert!(!r.sim.wants_text(), "after {pre} and Esc");
    }
    for pre in ["d", "y", "g", "z", "2"] {
        r.keys(pre);
        assert!(!r.sim.wants_text(), "after {pre}: a motion is a command");
        r.keys("<Esc>");
    }
    r.keys("v");
    assert!(!r.sim.wants_text(), "visual mode");
}

#[test]
fn gui_5_the_command_line_and_the_search_prompt_are_text() {
    let mut r = rig_with_tree();
    r.keys(":");
    assert!(r.sim.wants_text(), "the command line");
    r.keys("<Esc>");
    assert!(!r.sim.wants_text());
    r.keys("/");
    assert!(r.sim.wants_text(), "the search prompt");
    r.keys("<Esc>");
    assert!(!r.sim.wants_text());
}

#[test]
fn gui_5_the_picker_s_query_is_text_while_it_is_open() {
    let mut r = rig_with_tree();
    r.keys("<C-p>");
    assert!(r.sim.wants_text(), "the query of the file picker");
    r.keys("<Esc>");
    assert!(!r.sim.wants_text(), "closed");
    r.keys(" ");
    r.pause();
    assert!(!r.sim.wants_text(), "which-key waits for a command");
}

#[test]
fn gui_5_the_tree_has_commands_until_its_search_is_typed() {
    let mut r = rig_with_tree();
    r.keys("<C-n>");
    assert!(!r.sim.wants_text(), "the tree has the focus: j and k are commands");
    r.keys("/");
    assert!(r.sim.wants_text(), "its search is typed");
    r.keys("b");
    assert!(r.sim.wants_text());
    r.keys("<Esc>");
    assert!(!r.sim.wants_text(), "and over");
}

#[test]
fn gui_5_with_no_file_open_nothing_is_text_until_a_picker_asks_for_it() {
    let mut r = rig("g5-none", &["a.txt=1\n"], &[]);
    assert!(!r.sim.wants_text());
    r.keys("<C-p>");
    assert!(r.sim.wants_text());
}
