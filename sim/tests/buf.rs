//! BUF-1, BUF-2, BUF-3: several files, each a complete si App: opening, switching, closing,
//! saving, the disk changing under a file that is not on screen, and the snapshots of all of them.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::{Effect, Event};
use si::core::vi::Mode;
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

/// `two` with b.txt and then a.txt modified ("ne", and "1" in b.txt) and a conflict with the disk
/// in a.txt: mine is "ne", the disk's is "ONE". a.txt is the current file, with one hunk open.
fn conflicted(name: &str) -> Rig {
    let mut r = two(name);
    r.keys("x<Tab>x");
    fs::write(r.cwd.join("a.txt"), "ONE\ntwo\n").unwrap();
    let a = r.sim.bufs[0].id;
    r.send(SimEvent::App(a, disk(b"ONE\ntwo\n")));
    assert_eq!((r.sim.cur, r.app().buf.hunks.len()), (0, 1));
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
    r.send(SimEvent::App(a, disk(b"ONE\ntwo\n")));
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
    r.send(SimEvent::App(b, disk(b"B1\nB2\nB3\n")));
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
fn buf_1_the_answer_of_a_save_or_a_reload_asks_for_the_watch_of_its_own_file() {
    // PROP-1: what a watcher said while a save was in flight cannot be ordered against it, so
    // the shell is asked to watch (and read) the file again after the answer.
    let mut r = two("b1-watch");
    let watched = |r: &Rig| -> Vec<(usize, String)> {
        let w = r.log.iter();
        w.filter_map(|e| match e {
            SimEffect::App(id, Effect::Watch(p)) => Some((*id, p.clone())),
            _ => None,
        })
        .collect()
    };
    r.log.clear();
    r.keys("x:w\n"); // b.txt
    assert_eq!(watched(&r), [(1, "b.txt".to_string())], "one file, once");
    r.log.clear();
    r.keys("<Tab>x:w!\n"); // a.txt
    assert_eq!(watched(&r), [(0, "a.txt".to_string())]);
    r.log.clear();
    r.keys("x:e!\n"); // a reload is read again and watched
    assert_eq!(watched(&r), [(0, "a.txt".to_string())]);
    r.log.clear();
    r.keys(":e c.txt\n"); // a new file is watched by the shell when it opens it: `Open`, not `Watch`
    assert!(watched(&r).is_empty() && r.open_files().len() == 3);
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

#[test]
fn buf_2_tab_and_shift_tab_do_not_switch_in_review() {
    let mut r = conflicted("b2-review");
    r.keys("]c");
    assert_eq!(r.app().vi.mode, Mode::Review);
    r.send(SimEvent::BackTab);
    assert_eq!((r.sim.cur, r.app().vi.mode), (0, Mode::Review), "dropped, and REVIEW stays");
    r.keys("<Tab>");
    assert_eq!(r.sim.cur, 0, "Tab went to the App, and not to the other file");
    assert_eq!(r.app().vi.mode, Mode::Normal, "where any key but y n Y N ends REVIEW (PROP-8)");
    // From the panel, with the App in REVIEW: the same.
    r.keys("]c<C-n>");
    assert!(r.sim.panel.focus && r.app().vi.mode == Mode::Review);
    r.keys("<Tab>");
    r.send(SimEvent::BackTab);
    assert_eq!((r.sim.cur, r.app().vi.mode), (0, Mode::Review));
    // A hunk that is open does not stop Tab: only REVIEW (and a pending key) does.
    r.keys("<Esc><Esc>");
    assert_eq!((r.app().vi.mode, r.app().buf.hunks.len()), (Mode::Normal, 1));
    r.keys("<Tab>");
    assert_eq!(r.sim.cur, 1);
    // The other file is not in REVIEW, so it switches back.
    r.send(SimEvent::BackTab);
    assert_eq!(r.sim.cur, 0);
}

#[test]
fn buf_3_wa_leaves_out_a_file_with_a_conflict_with_the_disk_names_it_and_saves_the_rest() {
    let mut r = conflicted("b3-wa-conflict");
    r.keys(":wa\n");
    assert_eq!(fs::read_to_string(r.cwd.join("b.txt")).unwrap(), "1\nB2\n", "the rest was saved");
    assert_eq!(fs::read_to_string(r.cwd.join("a.txt")).unwrap(), "ONE\ntwo\n", "mine was not");
    assert_eq!(
        r.status(),
        "not saved: conflicts with the disk in a.txt (]c, :w! keeps yours)  1/2"
    );
    assert!(r.sim.bufs[0].app.buf.dirty() && !r.sim.bufs[1].app.buf.dirty());
    assert_eq!(r.sim.bufs[0].app.buf.hunks.len(), 1, "and the hunk is still there");
    // Both files in conflict: both are named, in the order they are open, and nothing is saved.
    r.keys("<Tab>x");
    let b = r.sim.bufs[1].id;
    fs::write(r.cwd.join("b.txt"), "B1!\nB2\n").unwrap();
    r.send(SimEvent::App(b, disk(b"B1!\nB2\n")));
    assert!(r.sim.bufs.iter().all(|b| b.app.buf.hunks.len() == 1));
    r.keys(":wa\n");
    assert_eq!(fs::read_to_string(r.cwd.join("b.txt")).unwrap(), "B1!\nB2\n");
    assert!(r.status().starts_with("not saved: conflicts with the disk in a.txt, b.txt"));
    // `:w!` keeps mine in the current file; then `:wa` has only the other one to refuse.
    r.keys(":w!\n");
    assert_eq!(fs::read_to_string(r.cwd.join("b.txt")).unwrap(), "\nB2\n", "mine");
    r.keys("<Tab>:wa\n");
    assert!(r.status().contains("conflicts with the disk in a.txt ("), "{}", r.status());
    r.keys(":w!\n:wa\n");
    assert_eq!(fs::read_to_string(r.cwd.join("a.txt")).unwrap(), "ne\ntwo\n");
    assert_eq!(r.status(), "no modified files  1/2");
}

#[test]
fn buf_3_qa_stays_while_a_file_has_unresolved_proposals_names_them_and_qa_bang_discards() {
    let mut r = conflicted("b3-qa-hunks");
    r.keys(":qa\n");
    assert!(r.quit.is_none());
    assert_eq!(r.status(), "unresolved proposals in a.txt (]c to review, :qa! discards)  1/2");
    // Deciding the hunk opens the door a little: what is modified is told next (SAFE-7, BUF-3).
    r.keys("]cn");
    assert!(r.sim.bufs.iter().all(|b| b.app.buf.hunks.is_empty()));
    r.keys(":qa\n");
    assert!(r.quit.is_none());
    assert_eq!(r.status(), "no write since last change in a.txt, b.txt (:qa! to discard)  1/2");
    // `:qa!` leaves with proposals open too, and writes nothing.
    let mut r = conflicted("b3-qa-bang");
    r.log.clear();
    r.keys(":qa!\n");
    assert_eq!(r.quit, Some(0));
    assert!(r.log.iter().any(|e| is_unsnapshot(e, 0)) && r.log.iter().any(|e| is_unsnapshot(e, 1)));
    assert_eq!(fs::read_to_string(r.cwd.join("a.txt")).unwrap(), "ONE\ntwo\n");
    assert_eq!(fs::read_to_string(r.cwd.join("b.txt")).unwrap(), "B1\nB2\n");
}

#[test]
fn buf_3_each_file_keeps_si_s_rules_for_proposals_when_it_is_closed() {
    let mut r = conflicted("b3-q-hunks");
    for quit in [":q\n", "q", ":x\n", "ZZ", ":wq\n"] {
        r.keys(quit);
        assert_eq!(r.open_files(), ["a.txt", "b.txt"], "{quit:?} must not close it");
    }
    assert_eq!(fs::read_to_string(r.cwd.join("a.txt")).unwrap(), "ONE\ntwo\n", "and not save");
    r.keys(":q\n");
    assert_eq!(
        r.status(),
        "unresolved proposals: ]c to review, :q! discards  1/2",
        "said instead of `no write since last change`"
    );
    r.keys(":q!\n");
    assert_eq!(r.open_files(), ["b.txt"], ":q! discards the proposals and closes only this file");
    assert!(r.quit.is_none());
}

// ---- on si's M4 and M5: the guarantees hold for every file ----

#[test]
fn buf_1_a_file_opens_where_it_was_closed_unless_keys_say_otherwise() {
    let long: String = (1..=40).map(|i| format!("line {i}\n")).collect();
    let mut r = rig("b1-place", &[&format!("a.txt={long}"), "b.txt=one\ntwo\nthree\n"], &[]);
    // Where the files were left, in si's own places (CLI-6): the shell finds it while it reads.
    let (ka, kb) = (key_of_in(&r, "a.txt"), key_of_in(&r, "b.txt"));
    let mut all = journal::places(&r.state);
    journal::touch(&mut all, ka, 30, 2);
    journal::touch(&mut all, kb, 2, 3);
    journal::save_places(&r.state, &all).unwrap();
    r.keys("<CR>");
    assert_eq!(r.status(), "a.txt  31:3  NORMAL");
    r.keys("<C-n>j<CR>");
    assert_eq!(r.status(), "b.txt  3:4  NORMAL  2/2");
    // The keys of a `+cmd` or `file:line` win over the place, and only for the file they are for.
    let mut r = rig("b1-place-keys", &["a.txt=one\ntwo\nthree\n", "b.txt=x\ny\nz\n"], &["a.txt:2"]);
    let mut all = journal::places(&r.state);
    journal::touch(&mut all, key_of_in(&r, "b.txt"), 2, 0);
    journal::save_places(&r.state, &all).unwrap();
    assert_eq!(r.status(), "a.txt  2:1  NORMAL");
    r.keys(":e b.txt\n");
    assert_eq!(r.status(), "b.txt  3:1  NORMAL  2/2", "another file is opened at its place");
}

fn key_of_in(r: &Rig, f: &str) -> String {
    si::io::file::resolve(&r.cwd.join(f).to_string_lossy()).to_string_lossy().into_owned()
}

#[test]
fn buf_3_leaving_remembers_where_each_file_was_left_in_si_s_way() {
    let mut r = two("b3-remember");
    r.keys("j<Tab>l");
    r.log.clear();
    r.keys(":qa\n");
    let places: Vec<_> = r
        .log
        .iter()
        .filter_map(|e| match e {
            SimEffect::App(id, Effect::Remember { path, line, col }) => {
                Some((*id, path.clone(), *line, *col))
            }
            _ => None,
        })
        .collect();
    assert_eq!(r.quit, Some(0));
    assert_eq!(places.len(), 2, "{places:?}");
    assert!(places.contains(&(r.sim.bufs[1].id, "b.txt".into(), 1, 0)));
    assert!(places.contains(&(r.sim.bufs[0].id, "a.txt".into(), 0, 1)));
    // A signal, and `:cq`, give them too (as they give each file its last snapshot).
    let mut r = two("b3-remember-sig");
    r.log.clear();
    r.send(SimEvent::Signal(15));
    let n =
        r.log.iter().filter(|e| matches!(e, SimEffect::App(_, Effect::Remember { .. }))).count();
    assert_eq!((n, r.quit), (2, Some(143)));
}

#[test]
fn buf_1_every_file_runs_the_hook_of_the_environment_and_knows_the_working_directory() {
    let mut r =
        rig_env("b1-hook", &FILES, &[], (80, 24), |e| e.hook = Some("fcitx5-remote -c".into()));
    r.keys("<CR>");
    assert_eq!(r.app().cwd.as_deref(), Some(r.cwd.to_string_lossy().as_ref()));
    assert_eq!(r.app().hook.as_deref(), Some("fcitx5-remote -c"));
    r.keys("<C-n>j<CR>");
    assert_eq!(r.open_files(), ["a.txt", "b.txt"]);
    assert_eq!(r.app().hook.as_deref(), Some("fcitx5-remote -c"), "every file's App");
    let hooks =
        |r: &Rig| r.log.iter().filter(|e| matches!(e, SimEffect::App(_, Effect::Hook(_)))).count();
    let before = hooks(&r);
    r.keys("ix<Esc>"); // entering Normal mode again (VI-11)
    assert_eq!(hooks(&r), before + 1);
}

#[test]
fn buf_1_a_filter_belongs_to_the_file_it_was_started_in() {
    let mut r = two("b1-filter");
    r.keys(":%!tr a-z A-Z\n"); // in b.txt
    let b = r.sim.bufs[1].id;
    let spawned = r.log.iter().find_map(|e| match e {
        SimEffect::App(id, Effect::Spawn { cmd, input }) => Some((*id, cmd.clone(), input.clone())),
        _ => None,
    });
    assert_eq!(spawned, Some((b, "tr a-z A-Z".into(), b"B1\nB2\n".to_vec())));
    r.keys("<Tab>");
    // The answer finds b.txt wherever the user is, and a.txt knows nothing of it.
    let done = Event::FilterDone(Some(0), b"b1\nB2\n".to_vec(), vec![], far());
    r.send(SimEvent::App(b, done));
    assert_eq!(r.sim.bufs[1].app.buf.hunks.len(), 1);
    assert!(r.sim.bufs[0].app.buf.hunks.is_empty());
    // An answer for a file that is not open (ids are never reused) is dropped.
    r.send(SimEvent::App(77, Event::FilterDone(Some(0), vec![], vec![], far())));
    assert_eq!(r.open_files(), ["a.txt", "b.txt"]);
}

#[test]
fn buf_1_a_read_of_the_disk_older_than_a_save_s_answer_is_dropped_by_that_file() {
    // PROP-1, per file: a watcher's read that began before the save was done must not undo it, and
    // a read that began after it is taken in.
    let mut r = two("b1-seq");
    let a = r.sim.bufs[0].id;
    r.keys("<Tab>x"); // a.txt: "ne"
    let began = si::io::file::seq(); // a watcher's read starts here: it sees the old file
    r.keys(":w\n");
    assert_eq!(fs::read_to_string(r.cwd.join("a.txt")).unwrap(), "ne\ntwo\n");
    let read = |text: &str, seq| {
        let hash = Some(si::core::buffer::hash(text.as_bytes()));
        Event::Disk { bytes: Some(text.into()), hash, deadline: far(), seq }
    };
    r.send(SimEvent::App(a, read("one\ntwo\n", began)));
    assert_eq!(r.text(), "ne\ntwo");
    assert!(r.app().buf.marks.is_empty() && r.app().buf.hunks.is_empty() && !r.app().buf.dirty());
    r.send(SimEvent::App(a, read("ne\ntwo\nthree\n", si::io::file::seq())));
    assert_eq!(r.text(), "ne\ntwo\nthree", "a newer read is a change on disk");
    assert!(!r.app().buf.marks.is_empty());
}

#[test]
fn buf_1_the_seconds_of_a_running_filter_are_counted_on_the_screen_as_the_ticks_come() {
    let mut r = two("b1-seconds");
    r.keys(":%!sleep 100\n");
    assert!(r.status().contains("filter 0s · Ctrl-c cancel"), "{}", r.status());
    r.send(SimEvent::Tick(1000));
    assert!(r.status().contains("filter 1s · Ctrl-c cancel"), "{}", r.status());
    r.send(SimEvent::Tick(2000));
    assert!(r.status().contains("filter 2s"), "{}", r.status());
    // A tick that changes nothing on the screen draws nothing.
    let mut quiet = two("b1-quiet");
    let before = quiet.log.len();
    let fx = quiet.sim.update(SimEvent::Tick(1000));
    assert!(fx.iter().all(|e| !matches!(e, SimEffect::Render(_))), "{fx:?}");
    assert_eq!(quiet.log.len(), before);
}
