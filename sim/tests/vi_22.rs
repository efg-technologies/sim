//! VI-22 in Sim: Alt-h/j/k/l move the lines of the text, and where a key is not the text's (the tree, a
//! picker, the leader, Ctrl-w, the letters of the tabs) an Alt key is Esc and the key, as it always was.
mod common;
use common::*;

fn text(name: &str) -> Rig {
    rig(name, &["f.txt=one\ntwo\nthree\n", "g.txt=x\n"], &["f.txt"])
}

#[test]
fn vi_22_alt_j_moves_the_line_of_the_text() {
    let mut r = text("v22-text");
    r.keys("<M-j>");
    assert_eq!(r.text(), "two\none\nthree");
    assert!(r.status().contains("2:1"), "{}", r.status());
}

#[test]
fn vi_22_in_the_tree_an_alt_key_is_esc_and_the_key() {
    let mut r = text("v22-tree");
    r.keys("<C-w>w");
    assert!(r.sim.panel.focus);
    // Esc gives the text the focus, and `j` moves down in it; no line moves
    r.keys("<M-j>");
    assert!(!r.sim.panel.focus);
    assert_eq!(r.text(), "one\ntwo\nthree");
    assert!(r.status().contains("2:1"), "{}", r.status());
}

#[test]
fn vi_22_after_the_leader_or_ctrl_w_an_alt_key_is_esc_and_the_key() {
    for first in ["<Space>", "<C-w>"] {
        let mut r = text("v22-lead");
        r.keys(first).keys("<M-j>");
        assert_eq!(r.text(), "one\ntwo\nthree", "{first}");
        assert!(r.status().contains("2:1"), "{first}: {}", r.status());
    }
}
