//! BUF-1, BUF-2, BUF-3: several files, each a complete si App: opening, switching, closing,
//! saving, the disk changing under a file that is not on screen, and the snapshots of all of them.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::{Effect, Event};
use si::io::journal::{self, Snapshot};
use sim::sim::{SimEffect, SimEvent};
use std::fs;

/// Three small files. The panel is open and has the focus at the start, on `a.txt`.
const FILES: [&str; 3] = ["a.txt=one\ntwo\n", "b.txt=B1\nB2\n", "c.txt=C1\n"];

/// A Rig on FILES with a.txt and b.txt open (b.txt is the current one) and the focus in the text.
fn two(name: &str) -> Rig {
    let mut r = rig(name, &FILES, &[]);
    r.keys("<CR>");
    r.keys("<C-n>j<CR>");
    assert_eq!(r.open_files(), ["a.txt", "b.txt"]);
    r
}

fn is_snapshot(e: &SimEffect, id: usize) -> bool {
    matches!(e, SimEffect::App(i, Effect::Snapshot { .. }) if *i == id)
}

fn is_unsnapshot(e: &SimEffect, id: usize) -> bool {
    matches!(e, SimEffect::App(i, Effect::Unsnapshot) if *i == id)
}

#[test]
fn buf_1_opening_a_file_from_the_panel_reads_it_into_a_new_buffer() {
    let mut r = rig("b1-open", &FILES, &[]);
    r.keys("<CR>");
    assert_eq!(r.open_files(), ["a.txt"]);
    assert_eq!(r.text(), "one\ntwo");
    assert!(r.sim.panel.open && !r.sim.panel.focus, "the panel stays, the focus moves to the text");
    assert_eq!(r.editor(), ["   1 one", "   2 two"]);
    r.keys("<C-n>j<CR>");
    assert_eq!((r.open_files(), r.sim.cur), (["a.txt", "b.txt"].map(String::from).to_vec(), 1));
    assert_eq!(r.text(), "B1\nB2");
}

#[test]
fn buf_1_opening_a_file_that_is_open_switches_to_it() {
    let mut r = two("b1-again");
    r.keys("x"); // B1 -> 1: only b.txt is modified
    r.keys("<C-n>k<CR>");
    assert_eq!(r.open_files(), ["a.txt", "b.txt"], "no third buffer");
    assert_eq!(r.sim.cur, 0);
    assert!(!r.sim.panel.focus);
    r.keys("<C-n>j<CR>");
    assert_eq!(r.sim.cur, 1);
    assert_eq!(r.text(), "1\nB2", "it is the same buffer, with its edit");
    assert!(!r.log.iter().any(|e| matches!(e, SimEffect::Open(2, _))), "and it was not read again");
}

#[test]
fn buf_1_each_file_has_its_own_undo_modified_flag_and_cursor() {
    let mut r = two("b1-own");
    r.keys("jx"); // b.txt: 2:1
    r.keys("<Tab>");
    r.keys("lx"); // a.txt: "one" -> "oe"
    assert_eq!((r.text(), r.app().buf.dirty()), ("oe\ntwo".into(), true));
    r.keys("<Tab>");
    assert_eq!(r.text(), "B1\n2");
    r.keys("u");
    assert_eq!(r.text(), "B1\nB2", "u undoes b.txt only");
    assert!(!r.app().buf.dirty());
    r.keys("<Tab>");
    assert_eq!(r.text(), "oe\ntwo");
    assert!(r.app().buf.dirty());
}

#[test]
fn buf_1_e_with_a_name_opens_another_file_even_when_this_one_is_modified() {
    let mut r = rig("b1-e", &FILES, &[]);
    r.keys("<CR>x:e b.txt\n");
    assert_eq!(r.open_files(), ["a.txt", "b.txt"]);
    assert_eq!(r.sim.cur, 1);
    r.keys("<Tab>");
    assert_eq!(r.text(), "ne\ntwo", "nothing was discarded");
    r.keys(":e! c.txt\n");
    assert_eq!(r.open_files(), ["a.txt", "b.txt", "c.txt"]);
    r.keys(":e a.txt\n");
    assert_eq!((r.open_files().len(), r.sim.cur), (3, 0), "an open file is switched to");
    assert_eq!(r.text(), "ne\ntwo");
}

#[test]
fn buf_1_e_without_a_name_reloads_this_buffer() {
    let mut r = rig("b1-reload", &FILES, &[]);
    r.keys("<CR>x:e\n");
    assert_eq!(r.status(), "no write since last change (:e! to discard)");
    r.keys(":e!\n");
    assert_eq!(r.text(), "one\ntwo");
    assert_eq!(r.open_files(), ["a.txt"]);
}

#[test]
fn buf_1_e_with_a_directory_reveals_it_in_the_panel() {
    let mut r = rig("b1-dir", &["a/b/c/d.txt", "a/e.txt", "f.txt=ff\n"], &["f.txt"]);
    r.keys(":e a/b\n");
    assert_eq!(r.open_files(), ["f.txt"], "no buffer");
    assert!(r.sim.panel.open && r.sim.panel.focus);
    assert_eq!(
        r.panel(),
        ["▾ a/", "  ▾ b/", "    ▸ c/", "    e.txt", "• f.txt"],
        "f.txt is likely"
    );
    assert_eq!(r.selected().as_deref(), Some("  ▾ b/"));
    r.keys("<Esc>:e a/b/c\n");
    assert_eq!(r.selected().as_deref(), Some("    ▾ c/"));
    assert!(r.panel().contains(&"        d.txt".to_string()), "{:?}", r.panel());
    r.keys("<Esc>:e /\n");
    assert!(r.status().contains("not under"), "outside the tree: {}", r.status());
}

#[test]
fn buf_1_two_names_for_one_file_are_one_buffer() {
    let mut r = rig("b1-link", &FILES, &[]);
    std::os::unix::fs::symlink("a.txt", r.cwd.join("link.txt")).unwrap();
    r.keys("<CR>:e link.txt\n");
    assert_eq!(
        r.open_files(),
        ["a.txt"],
        "the journal could not tell two buffers on one file apart"
    );
    assert!(r.log.iter().any(|e| matches!(e, SimEffect::Close(1))), "the second was dropped");
}

#[test]
fn buf_1_a_disk_change_to_a_file_that_is_not_on_screen_is_merged_there() {
    let mut r = two("b1-bg");
    let (a, b) = (r.sim.bufs[0].id, r.sim.bufs[1].id);
    let before = r.screen.clone();
    r.send(SimEvent::App(a, Event::Disk { bytes: Some(b"ONE\ntwo\n".to_vec()), deadline: far() }));
    assert_eq!(r.sim.cur, 1, "the file on screen is still b.txt");
    assert_eq!(r.text(), "B1\nB2");
    assert_eq!(r.sim.bufs[0].app.buf.text.to_string(), "ONE\ntwo", "a.txt took it in");
    assert!(!r.sim.bufs[0].app.buf.marks.is_empty(), "and marked it");
    // Nothing on screen moved; the only news is the change color on a.txt in the panel (MARK-1).
    let mut after = r.screen.rows.clone();
    after[0].spans.retain(|s| s.1 != si::core::view::Style::Change);
    assert_eq!(after, before.rows, "nothing on screen moved");
    assert!(r.screen.rows[0].spans.iter().any(|s| s.1 == si::core::view::Style::Change));
    // A change to the file on screen merges there, and only there.
    r.send(SimEvent::App(
        b,
        Event::Disk { bytes: Some(b"B1\nB2\nB3\n".to_vec()), deadline: far() },
    ));
    assert_eq!(r.text(), "B1\nB2\nB3");
    assert_eq!(r.sim.bufs[0].app.buf.text.to_string(), "ONE\ntwo");
    // Switching shows the merged file and its marks.
    r.keys("<Tab>");
    assert!(r.status().contains("1 change on disk"), "{}", r.status());
}

#[test]
fn buf_1_a_save_in_the_background_is_checked_against_the_disk_of_its_own_file() {
    let mut r = two("b1-cas");
    r.keys("<Tab>x"); // a.txt: modified
    r.keys("<Tab>");
    fs::write(r.cwd.join("a.txt"), "one\nTWO\n").unwrap(); // somebody else, and nobody has noticed yet
    r.keys(":wa\n");
    assert_eq!(
        fs::read_to_string(r.cwd.join("a.txt")).unwrap(),
        "one\nTWO\n",
        "nothing was overwritten"
    );
    assert_eq!(r.sim.bufs[0].app.buf.text.to_string(), "ne\nTWO", "its merge happened in a.txt");
    assert_eq!(r.sim.cur, 1, "b.txt stayed on screen");
    assert!(
        r.sim.bufs[0]
            .app
            .msg
            .as_deref()
            .unwrap()
            .starts_with("not saved: the file changed on disk")
    );
}

#[test]
fn buf_1_an_unsaved_snapshot_is_recovered_for_the_file_that_has_one() {
    let mut r = rig("b1-recover", &FILES, &[]);
    let key = si::io::file::resolve(&r.cwd.join("b.txt").to_string_lossy())
        .to_string_lossy()
        .into_owned();
    let dead = u32::MAX / 2;
    let s = Snapshot {
        path: key.clone(),
        pid: dead,
        time_ms: 1,
        base: b"B1\nB2".to_vec(),
        text: b"B1\nB2\nB3".to_vec(),
    };
    journal::write(&r.state, &s).unwrap();
    r.keys("<CR>");
    assert_eq!(r.text(), "one\ntwo", "a.txt has none");
    r.keys("<C-n>j<CR>");
    assert_eq!(r.text(), "B1\nB2\nB3");
    assert!(r.app().buf.dirty());
    assert!(r.status().starts_with("recovered unsaved changes from"), "{}", r.status());
    let held = r.log.iter().find_map(|e| match e {
        SimEffect::Hold(id, k, pid) => Some((*id, k.clone(), *pid)),
        _ => None,
    });
    assert_eq!(held, Some((1, key, dead)), "the dead session's snapshot is ours to delete");
    // a.txt is left as it was.
    r.keys("<Tab>");
    assert!(!r.app().buf.dirty());
}

#[test]
fn buf_1_a_signal_snapshots_every_modified_file_and_exits() {
    let mut r = two("b1-signal");
    r.keys("x<Tab>x"); // both are modified
    r.keys("<C-n>j<CR>"); // and a third, clean one is open too
    assert_eq!(r.open_files().len(), 3);
    r.log.clear();
    r.send(SimEvent::Signal(15));
    let snaps: Vec<_> = r
        .log
        .iter()
        .filter(|e| is_snapshot(e, 0) || is_snapshot(e, 1) || is_snapshot(e, 2))
        .collect();
    assert_eq!(snaps.len(), 2, "one for each modified file: {:?}", r.log);
    assert!(r.log.iter().any(|e| is_snapshot(e, 0)) && r.log.iter().any(|e| is_snapshot(e, 1)));
    assert!(!r.log.iter().any(|e| is_unsnapshot(e, 0)), "a signal keeps the snapshots");
    assert_eq!(r.quit, Some(128 + 15));
    // The text of each snapshot is the buffer's.
    let texts: Vec<String> = r
        .log
        .iter()
        .filter_map(|e| match e {
            SimEffect::App(_, Effect::Snapshot { text, .. }) => Some(text.to_string()),
            _ => None,
        })
        .collect();
    assert!(
        texts.contains(&"ne\ntwo".to_string()) && texts.contains(&"1\nB2".to_string()),
        "{texts:?}"
    );
}

#[test]
fn buf_1_the_ticks_snapshot_each_modified_file_once_a_second() {
    let mut r = two("b1-tick");
    r.keys("x<Tab>x");
    r.log.clear();
    r.send(SimEvent::Tick(1000));
    assert_eq!(r.log.iter().filter(|e| is_snapshot(e, 0) || is_snapshot(e, 1)).count(), 2);
    r.log.clear();
    r.send(SimEvent::Tick(2000));
    assert!(r.log.is_empty(), "nothing changed since: {:?}", r.log);
}

#[test]
fn buf_2_tab_and_shift_tab_switch_to_the_next_and_the_previous_file_and_wrap() {
    let mut r = rig("b2-tab", &FILES, &[]);
    r.keys("<CR><C-n>j<CR><C-n>j<CR>");
    assert_eq!((r.open_files().len(), r.sim.cur), (3, 2));
    r.keys("<Tab>");
    assert_eq!(r.sim.cur, 0, "wraps to the first");
    r.keys("<Tab>");
    assert_eq!(r.sim.cur, 1);
    r.send(SimEvent::BackTab);
    r.send(SimEvent::BackTab);
    assert_eq!(r.sim.cur, 2, "wraps to the last");
    // From the panel too, and the focus stays where it is.
    r.keys("<C-n>");
    r.keys("<Tab>");
    assert_eq!((r.sim.cur, r.sim.panel.focus), (0, true));
}

#[test]
fn buf_2_tab_only_switches_in_normal_mode_with_nothing_pending() {
    let mut r = two("b2-normal");
    for pending in ["i", "v", "V", "d", "2", "g", "f", "r", ":", "/", "y"] {
        r.keys(pending);
        r.keys("<Tab>");
        assert_eq!(r.sim.cur, 1, "after {pending:?} the key went to the App");
        r.keys("<Esc><Esc>");
        r.keys("<Esc>");
    }
    assert_eq!(r.sim.cur, 1);
    r.keys("<Tab>");
    assert_eq!(r.sim.cur, 0, "with nothing pending it switches");
    // In Insert mode a Tab is text.
    r.keys("i<Tab><Esc>");
    assert_eq!(r.sim.cur, 0);
    assert!(r.text().starts_with("    one"), "{:?}", r.text());
    // Shift-Tab follows the same rule.
    r.keys("d");
    r.send(SimEvent::BackTab);
    assert_eq!(r.sim.cur, 0);
}

#[test]
fn buf_2_the_status_line_shows_the_position_when_more_than_one_file_is_open() {
    let mut r = rig("b2-status", &FILES, &[]);
    r.keys("<CR>");
    assert_eq!(r.status(), "a.txt  1:1  NORMAL");
    r.keys("<C-n>j<CR>");
    assert_eq!(r.status(), "b.txt  1:1  NORMAL  2/2");
    r.keys("x");
    assert_eq!(r.status(), "b.txt  1:1  NORMAL  modified  2/2");
    r.keys("<C-n>j<CR><Tab>");
    assert_eq!(r.status(), "a.txt  1:1  NORMAL  1/3");
    // A message keeps the position too, and so does the panel's own line.
    r.keys(":nope\n");
    assert_eq!(r.status(), "not an editor command: nope  1/3");
    r.keys("l<C-n>");
    assert_eq!(r.status(), "c.txt  TREE  1/3");
}

#[test]
fn buf_2_panel_and_editor_are_one_frame_with_the_position_in_the_status_line() {
    let files =
        ["src/lib.rs=fn a() {}\nfn b() {}\n", "src/main.rs=fn main() {}\n", "a.txt", "z.txt=Z\n"];
    let mut r = rig_sized("b2-golden", &files, &[], (60, 10));
    r.keys("lj<CR>"); // src/lib.rs
    r.keys("<C-n>j<CR>"); // src/main.rs
    r.keys("<C-n>jj<CR>"); // z.txt
    r.keys("<Tab><Tab>");
    assert_eq!(r.status(), "src/main.rs  1:1  NORMAL  2/3");
    golden("buf_2_two_files_and_the_tree", &dump(&r.screen));
    r.keys("<C-n>");
    golden("buf_2_the_tree_with_the_focus", &dump(&r.screen));
}

#[test]
fn buf_3_q_closes_the_current_file_and_the_next_one_shows() {
    let mut r = rig("b3-q", &FILES, &[]);
    r.keys("<CR><C-n>j<CR><C-n>j<CR>");
    r.keys("<Tab>:q\n"); // a.txt closes: b.txt is its neighbor
    assert_eq!(r.open_files(), ["b.txt", "c.txt"]);
    assert_eq!((r.sim.cur, r.status()), (0, "b.txt  1:1  NORMAL  1/2"));
    r.keys("<Tab>q"); // c.txt, the last in line: its neighbor is the one before
    assert_eq!(r.open_files(), ["b.txt"]);
    assert_eq!(r.status(), "b.txt  1:1  NORMAL");
    assert!(r.quit.is_none());
    assert!(
        r.log.iter().any(|e| matches!(e, SimEffect::Close(0)))
            && r.log.iter().any(|e| matches!(e, SimEffect::Close(2)))
    );
}

#[test]
fn buf_3_closing_a_modified_file_is_refused_and_zq_discards_it() {
    let mut r = rig("b3-refuse", &FILES, &[]);
    r.keys("<CR><C-n>j<CR>x");
    for quit in [":q\n", "q"] {
        r.keys(quit);
        assert_eq!(r.open_files().len(), 2, "{quit:?}");
    }
    assert_eq!(r.status(), "no write since last change (:q! to override)  2/2");
    r.log.clear();
    r.keys("ZQ");
    assert_eq!(r.open_files(), ["a.txt"]);
    assert!(r.log.iter().any(|e| is_unsnapshot(e, 1)), "the snapshot of what was discarded goes");
    assert_eq!(fs::read_to_string(r.cwd.join("b.txt")).unwrap(), "B1\nB2\n");
}

#[test]
fn buf_3_wq_x_and_zz_save_and_close() {
    for (i, quit) in [":wq\n", ":x\n", "ZZ"].into_iter().enumerate() {
        let mut r = two(&format!("b3-wq-{i}"));
        r.keys("x");
        r.keys(quit);
        assert_eq!(r.open_files(), ["a.txt"], "{quit:?}");
        assert_eq!(fs::read_to_string(r.cwd.join("b.txt")).unwrap(), "1\nB2\n", "{quit:?}");
        assert!(r.quit.is_none());
    }
}

#[test]
fn buf_3_closing_the_last_file_exits_with_the_code() {
    let mut r = rig("b3-last", &FILES, &["a.txt"]);
    r.keys(":q\n");
    assert_eq!(r.quit, Some(0));
    let mut r = rig("b3-last-wq", &FILES, &["a.txt"]);
    r.keys("x:wq\n");
    assert_eq!(r.quit, Some(0));
    assert_eq!(fs::read_to_string(r.cwd.join("a.txt")).unwrap(), "ne\ntwo\n");
}

#[test]
fn buf_3_with_no_file_open_q_quits() {
    let mut r = rig("b3-none", &FILES, &[]);
    r.keys("q"); // closes the panel
    assert!(r.quit.is_none() && !r.sim.panel.open);
    r.keys("q");
    assert_eq!(r.quit, Some(0));
}

#[test]
fn buf_3_qa_exits_when_nothing_is_modified_and_otherwise_names_the_files() {
    let mut r = two("b3-qa");
    r.keys("x<Tab>x");
    r.keys(":qa\n");
    assert!(r.quit.is_none());
    assert_eq!(r.status(), "no write since last change in a.txt, b.txt (:qa! to discard)  1/2");
    r.keys("<Tab>:w\n");
    r.keys("<Tab>:qa\n");
    assert!(r.quit.is_none(), "one is still modified");
    assert_eq!(r.status(), "no write since last change in a.txt (:qa! to discard)  1/2");
    r.keys(":w\n");
    r.keys(":qa\n");
    assert_eq!(r.quit, Some(0));
}

#[test]
fn buf_3_qa_bang_discards_everything_and_forgets_every_snapshot() {
    let mut r = two("b3-qabang");
    r.keys("x<Tab>x");
    r.log.clear();
    r.keys(":qa!\n");
    assert_eq!(r.quit, Some(0));
    assert!(r.log.iter().any(|e| is_unsnapshot(e, 0)) && r.log.iter().any(|e| is_unsnapshot(e, 1)));
    assert_eq!(fs::read_to_string(r.cwd.join("a.txt")).unwrap(), "one\ntwo\n");
    assert_eq!(fs::read_to_string(r.cwd.join("b.txt")).unwrap(), "B1\nB2\n");
}

#[test]
fn buf_3_wa_saves_every_modified_file_and_quits_nothing() {
    let mut r = two("b3-wa");
    r.keys("x<Tab>x"); // a.txt and b.txt
    r.keys("<C-n>j<CR>"); // c.txt: not modified
    r.keys(":wa\n");
    assert_eq!(fs::read_to_string(r.cwd.join("a.txt")).unwrap(), "ne\ntwo\n");
    assert_eq!(fs::read_to_string(r.cwd.join("b.txt")).unwrap(), "1\nB2\n");
    assert!(r.sim.bufs.iter().all(|b| !b.app.buf.dirty()));
    assert!(r.quit.is_none());
    r.keys(":wa\n");
    assert_eq!(r.status(), "no modified files  3/3");
    r.keys(":qa\n");
    assert_eq!(r.quit, Some(0));
}

#[test]
fn buf_3_cq_exits_with_1_and_keeps_the_snapshots_of_the_other_modified_files() {
    let mut r = two("b3-cq");
    r.keys("x<Tab>x"); // a.txt (current) and b.txt are modified
    r.log.clear();
    r.keys(":cq\n");
    assert_eq!(r.quit, Some(1));
    assert!(r.log.iter().any(|e| is_unsnapshot(e, 0)), "the file :cq was typed in is given up");
    assert!(r.log.iter().any(|e| is_snapshot(e, 1)), "the other one is snapshotted before exiting");
    assert!(!r.log.iter().any(|e| is_unsnapshot(e, 1)));
    assert_eq!(
        fs::read_to_string(r.cwd.join("b.txt")).unwrap(),
        "B1\nB2\n",
        "and nothing is saved"
    );
}

#[test]
fn buf_3_a_search_or_a_substitute_is_not_mistaken_for_an_all_command() {
    let mut r = two("b3-notall");
    r.keys("/qa\n");
    r.keys(":s/qa/wa/\n");
    r.keys(":e\n");
    assert!(r.quit.is_none());
    assert_eq!(r.open_files(), ["a.txt", "b.txt"]);
}
