//! GREP-1, GREP-3, GREP-4, GREP-5, GREP-6, GREP-7: the content search driven through sim's state
//! (`Sim::update`), with the shell's search and preview played by the test at once. Its results are
//! the list of the picker (PICK-1 to PICK-5), whose presentation `pick.rs` tests.
#![allow(clippy::unwrap_used, clippy::single_range_in_vec_init)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::grep::{Batch, Group, Line};
use sim::paint::{Look, Painter};
use sim::sim::{Pv, SimEffect, SimEvent};
use sim_core::core::app::Event;
use sim_core::core::view::{Shape, Style, width as cols};

/// The rows of the picker's list with their bold parts in `*…*`.
fn lit(r: &Rig) -> Vec<String> {
    let Some(w) = r.win() else { return vec![] };
    let mut out = vec![];
    for y in w.list.y..w.list.y + w.list.h {
        let cs: Vec<char> = r.screen.rows[usize::from(y)].text.chars().collect();
        let bold = |col: u16| {
            let on = |(rg, l): &(std::ops::Range<u16>, Look)| *l == Look::Bold && rg.contains(&col);
            r.shot.looks[usize::from(y)].iter().any(on)
        };
        let (mut text, mut col, mut on) = (String::new(), 0u16, false);
        for (k, &c) in cs.iter().enumerate() {
            let w_c = cols(&cs, k, usize::from(col)) as u16;
            if col >= w.list.x + w.list.w {
                break;
            }
            if col >= w.list.x {
                if bold(col) != on {
                    text.push('*');
                    on = !on;
                }
                text.push(c);
            }
            col += w_c;
        }
        text.extend(on.then_some('*'));
        out.push(text.trim_end().to_string());
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

fn grep_rig(name: &str, files: &[&str], args: &[&str]) -> Rig {
    rig(name, files, args)
}

const FILES: [&str; 3] = [
    "a.txt=alpha needle\nbeta\n",
    "sub/b.txt=gamma\nneedle again and Needle\n",
    "c.txt=nothing here\n",
];

fn held(r: &Rig) -> &sim::grep::Request {
    r.grep_held.last().unwrap()
}

#[test]
fn grep_1_ctrl_f_from_a_file_opens_the_picker_and_types_a_query() {
    let mut r = grep_rig("g1-open", &FILES, &["a.txt"]);
    assert!(!r.sim.panel.open);
    let status = r.status().to_string();
    r.keys("<C-f>");
    assert!(r.sim.picker_open() && !r.sim.panel.open, "the panel is not the picker's");
    assert!(r.float()[0].contains("Live grep"));
    let w = r.win().unwrap();
    assert_eq!(
        r.screen.cursor,
        (w.prompt.x + 3, w.prompt.y),
        "the cursor is at the end of the query"
    );
    assert_eq!(r.screen.shape, Shape::Bar);
    assert_eq!(r.status(), status, "the status line is the file's");
    r.keys("needle");
    assert_eq!(r.query(), "needle");
    assert_eq!(r.screen.cursor, (w.prompt.x + 3 + 6, w.prompt.y));
    assert_eq!(r.count(), "1 / 2");
    assert_eq!(r.list(), ["▎a.txt:1  alpha needle", " sub/b.txt:2  needle again and Needle"]);
    assert!(r.jobs.contains(&"grep"));
    // the results follow the keys: one more char narrows, Backspace widens
    r.keys("s");
    assert_eq!(r.count(), "0 / 0");
    assert_eq!(r.list(), [" no matches"]);
    r.keys("<BS>");
    assert_eq!(r.count(), "1 / 2");
}

#[test]
fn grep_1_ctrl_f_works_in_insert_mode_and_never_types() {
    let mut r = grep_rig("g1-insert", &FILES, &["a.txt"]);
    let text = r.text();
    r.keys("i");
    assert!(r.status().contains("INSERT"), "{}", r.status());
    r.keys("<C-f>needle");
    assert_eq!(r.count(), "1 / 2");
    assert_eq!(r.text(), text, "nothing was typed into the text");
    r.keys("<Esc>");
    assert!(r.status().contains("INSERT"), "back to the file as it was: {}", r.status());
    r.keys("x");
    assert!(r.text().starts_with('x'), "and the text takes keys again");
}

#[test]
fn grep_1_esc_and_backspace_on_an_empty_query_end_the_search() {
    let mut r = grep_rig("g1-end", &FILES, &["a.txt"]);
    let before = r.status().to_string();
    r.keys("<C-f>ne<Esc>");
    assert!(!r.sim.panel.open && !r.sim.picker_open(), "the panel was closed before");
    assert_eq!(r.status(), before);
    r.keys("<C-f>n<BS>");
    assert!(r.sim.picker_open());
    assert_eq!(r.query(), "");
    r.keys("<BS>");
    assert!(!r.sim.picker_open());
    assert_eq!(r.status(), before);
}

#[test]
fn grep_1_ctrl_f_again_changes_nothing_and_the_query_is_always_typed() {
    let mut r = grep_rig("g1-again", &FILES, &["a.txt"]);
    r.keys("<C-f>need<Down>");
    assert_eq!(r.screen.shape, Shape::Bar, "there is no browsing: the query is always typed");
    r.keys("<C-f>le");
    assert_eq!(r.query(), "needle");
    assert_eq!(r.count(), "1 / 2", "a new query starts on its first result");
    // the panel's keys are chars of the query here
    r.keys("<Down>/jk");
    assert_eq!(r.query(), "needle/jk");
}

#[test]
fn grep_1_a_name_search_waits_behind_the_picker_and_ctrl_n_and_ctrl_p_are_its_keys() {
    let mut r = grep_rig("g1-keys", &FILES, &[]);
    r.keys("/c.t");
    assert!(r.status().starts_with("/c.t  "));
    let tree = r.panel();
    r.keys("<C-f>needle");
    assert_eq!(r.count(), "1 / 2");
    let beside: Vec<String> = r
        .panel()
        .iter()
        .map(|l| l.chars().take(8).collect::<String>().trim_end().to_string())
        .collect();
    assert_eq!(beside[..3], tree[..3], "the tree is where it was, beside the window");
    r.keys("<C-n>");
    assert_eq!(r.count(), "2 / 2", "Ctrl-n is the picker's down");
    assert!(r.sim.panel.open, "and does not close the panel");
    r.keys("<C-p>");
    assert_eq!(r.count(), "1 / 2");
    assert!(r.float()[0].contains("Live grep"), "Ctrl-p is its up, not FIND-4's");
    r.keys("<Esc>");
    assert_eq!(r.panel(), tree, "and under it");
    assert!(
        r.status().starts_with("/c.t  1 match"),
        "the search of the tree is as it was: {}",
        r.status()
    );
}

#[test]
fn grep_1_tab_paste_and_the_mouse_do_nothing_while_it_is_on() {
    let mut r = grep_rig("g1-inert", &FILES, &["a.txt"]);
    r.keys(":e c.txt\n");
    assert_eq!(r.sim.cur, 1);
    r.keys("<C-f>needle<Down>");
    let (float, count) = (r.float(), r.count());
    r.keys("<Tab>");
    r.send(SimEvent::Term(Event::Paste("zzz".into())));
    r.click(3, 1);
    r.click(40, 3);
    r.wheel(true, 3, 3);
    assert_eq!((r.float(), r.count()), (float, count));
    assert_eq!(r.sim.cur, 1, "Tab did not switch files");
}

#[test]
fn grep_3_results_are_a_row_for_each_line_with_the_matches_bold_and_the_path_dim() {
    let mut r = grep_rig("g3-looks", &FILES, &[]);
    r.keys("<C-f>needle");
    assert_eq!(lit(&r), ["▎a.txt:1  alpha *needle*", " sub/b.txt:2  *needle* again and *Needle*"]);
    r.keys("<C-u>nEEdle");
    assert_eq!(r.count(), "0 / 0", "an uppercase letter makes it exact");
    r.keys("<C-u>Needle");
    assert_eq!(lit(&r), ["▎sub/b.txt:2  needle again and *Needle*"]);
    // the path and the number are the secondary style, the text is not
    let w = r.win().unwrap();
    let sec = |from: u16, to: u16| {
        let y = usize::from(w.list.y);
        let spans = &r.screen.rows[y].spans;
        spans.iter().any(|(rg, s)| {
            *s == Style::Secondary && rg.start <= w.list.x + from && rg.end >= w.list.x + to
        })
    };
    assert!(sec(1, 12) && !sec(14, 20));
}

#[test]
fn grep_3_files_are_ordered_by_the_history_and_then_by_path() {
    let files = ["a.txt=hit a\n", "b.txt=hit b\n", "c.txt=hit c\n", "d/e.txt=hit e\n"];
    let mut r = rig_env("g3-order", &files, &[], (80, 24), |env| {
        let root = env.cwd.display().to_string();
        env.history.record(&format!("{root}/c.txt"), NOW - 100);
        env.history.record(&format!("{root}/d/e.txt"), NOW - 5000);
        env.history.record(&format!("{root}/c.txt"), NOW - 9000);
    });
    r.keys("<C-f>hit");
    let paths: Vec<String> = r
        .list()
        .iter()
        .map(|l| l.trim_start_matches(['▎', ' ']).split(':').next().unwrap().to_string())
        .collect();
    assert_eq!(paths, ["c.txt", "d/e.txt", "a.txt", "b.txt"]);
}

#[test]
fn grep_3_batches_are_put_in_place_whatever_order_they_come_in_and_the_selection_stays() {
    let mut r = grep_rig("g3-batches", &["z.txt=x\n"], &[]);
    r.hold_grep = true;
    r.keys("<C-f>hit");
    let generation = held(&r).generation;
    let group = |p: &str, n: &[usize]| Group {
        path: p.into(),
        lines: n
            .iter()
            .map(|&n| Line { no: n, col: 0, text: format!("hit {p}{n}"), marks: vec![0..3] })
            .collect(),
    };
    let batch = |groups: Vec<Group>, done| {
        SimEvent::Grep(Batch { generation, groups, done, capped: false })
    };
    assert_eq!(r.count(), "0 / 0 ⠋", "searching");
    r.send(batch(vec![group("m.txt", &[1, 2, 3])], false));
    assert_eq!(r.list(), ["▎m.txt:1  hit m.txt1", " m.txt:2  hit m.txt2", " m.txt:3  hit m.txt3"]);
    assert_eq!(r.count(), "1 / 3 ⠋", "partial: the spinner runs");
    r.keys("<Down>");
    assert_eq!(r.list()[1], "▎m.txt:2  hit m.txt2");
    r.send(batch(vec![group("a.txt", &[7]), group("z.txt", &[9])], true));
    assert_eq!(
        r.list(),
        [
            " a.txt:7  hit a.txt7",
            " m.txt:1  hit m.txt1",
            "▎m.txt:2  hit m.txt2",
            " m.txt:3  hit m.txt3",
            " z.txt:9  hit z.txt9"
        ],
        "the selection stays on its line"
    );
    assert_eq!(r.count(), "3 / 5");
}

#[test]
fn grep_3_the_old_results_stay_until_the_first_batch_of_the_new_query_comes() {
    let mut r = grep_rig("g3-stale", &FILES, &[]);
    r.keys("<C-f>needle");
    assert_eq!(r.list()[0], "▎a.txt:1  alpha needle");
    r.hold_grep = true;
    r.keys("<BS>");
    assert!(r.count().ends_with('⠋'), "{}", r.count());
    assert_eq!(r.list()[0], "▎a.txt:1  alpha needle", "nothing blinked");
    r.keys("<CR>");
    assert!(r.open_files().is_empty(), "Enter does nothing while the results are the old query's");
    assert!(r.sim.picker_open());
    r.deliver_grep();
    assert_eq!(r.count(), "1 / 2");
    // and a query with no match: the end replaces the old results by nothing
    r.keys("zz");
    assert_eq!(r.list()[0], "▎a.txt:1  alpha needle");
    r.deliver_grep();
    assert_eq!(r.list(), [" no matches"]);
}

#[test]
fn grep_4_the_arrows_and_ctrl_n_and_ctrl_p_move_over_matching_lines_and_stop_at_the_ends() {
    let files = ["a.txt=hit 1\nx\nhit 3\n", "b.txt=hit b\n"];
    let mut r = grep_rig("g4-move", &files, &[]);
    r.keys("<C-f>hit");
    let sel = |r: &Rig| r.list().into_iter().find(|l| l.starts_with('▎')).unwrap();
    assert_eq!(sel(&r), "▎a.txt:1  hit 1", "the first line is selected");
    r.keys("<Down>");
    assert_eq!(sel(&r), "▎a.txt:3  hit 3");
    r.keys("<C-n>");
    assert_eq!(sel(&r), "▎b.txt:1  hit b");
    r.keys("<C-n><Down>");
    assert_eq!(sel(&r), "▎b.txt:1  hit b", "stops at the last line");
    r.keys("<C-p><C-p>");
    assert_eq!(sel(&r), "▎a.txt:1  hit 1");
    r.keys("<C-p><Up>");
    assert_eq!(sel(&r), "▎a.txt:1  hit 1", "and at the first");
    r.keys("<PageDown>");
    assert_eq!(sel(&r), "▎b.txt:1  hit b");
    r.keys("<PageUp>");
    assert_eq!(sel(&r), "▎a.txt:1  hit 1");
    // the keys of the panel are chars of the query
    r.keys("jk");
    assert_eq!(r.query(), "hitjk");
}

#[test]
fn grep_4_the_list_scrolls_to_keep_the_selected_row_in_view() {
    let text: String = (1..=60).map(|i| format!("hit {i}\n")).collect();
    let mut r = grep_rig("g4-scroll", &[&format!("a.txt={text}"), "b.txt=hit b\n"], &[]);
    r.keys("<C-f>hit");
    let rows = usize::from(r.win().unwrap().list.h);
    assert_eq!(r.list().len(), rows, "the list is full");
    for _ in 0..61 {
        r.keys("<C-n>");
        assert!(r.list().iter().any(|l| l.starts_with('▎')), "{:?}", r.list());
    }
    assert_eq!(r.list().last().unwrap(), "▎b.txt:1  hit b");
    assert_eq!(r.count(), "61 / 61");
    r.keys("<PageUp>");
    assert!(r.list().iter().any(|l| l.starts_with('▎')));
    for _ in 0..70 {
        r.keys("<C-p>");
    }
    assert_eq!(r.list()[0], "▎a.txt:1  hit 1", "back at the top the list begins at its first row");
}

#[test]
fn grep_4_the_picker_previews_the_file_under_the_selection_with_the_line_in_the_middle() {
    let long: String = (1..=100)
        .map(|i| if i == 60 { "the needle is here\n".to_string() } else { format!("line {i}\n") })
        .collect();
    let mut r =
        grep_rig("g4-preview", &["a.txt=one\ntwo\n", &format!("big.txt={long}")], &["a.txt"]);
    let text = r.text();
    r.keys("<C-f>needle");
    let w = r.win().unwrap();
    let pane = r.pane();
    let at = pane
        .iter()
        .position(|l| l.contains("the needle is here"))
        .unwrap_or_else(|| panic!("{pane:?}"));
    let mid = usize::from(w.pane.h) / 2;
    assert!(
        at.abs_diff(mid) <= 1,
        "the line is in the middle of {} rows, not at row {at}",
        w.pane.h
    );
    assert!(pane[at].contains(" 60 "), "with its number: {}", pane[at]);
    assert!(
        pane.iter().any(|l| l.contains("line 57")) && pane.iter().all(|l| !l.contains("line 1 "))
    );
    // the match is bold
    let y = usize::from(w.pane.y) + at;
    let row = &r.screen.rows[y].text;
    let col = row[..row.find("needle").unwrap()].chars().count() as u16;
    let bold = |(rg, l): &(std::ops::Range<u16>, Look)| {
        *l == Look::Bold && rg.start == col && rg.end == col + 6
    };
    assert!(r.shot.looks[y].iter().any(bold), "{col}: {:?}", r.shot.looks[y]);
    // the file that was open is not in the pane, and is not touched
    assert!(pane.iter().all(|l| !l.contains("one")));
    assert_eq!(r.text(), text);
}

#[test]
fn grep_4_the_preview_follows_the_selection_to_another_file_and_line() {
    let mut r = grep_rig("g4-follow", &FILES, &[]);
    r.keys("<C-f>needle");
    assert!(r.pane().iter().any(|l| l.contains("alpha needle")));
    r.keys("<Down>");
    let pane = r.pane();
    assert!(pane.iter().any(|l| l.contains("needle again and Needle")), "{pane:?}");
    assert!(pane.iter().all(|l| !l.contains("alpha")));
    assert!(pane.iter().any(|l| l.contains("gamma")), "the line before it is there too");
}

#[test]
fn grep_4_a_preview_is_not_an_open() {
    let mut r = grep_rig("g4-noopen", &FILES, &["a.txt"]);
    let history = |r: &Rig| r.jobs.iter().filter(|j| **j == "history").count();
    let (h, saved) = (r.history.clone(), history(&r));
    r.log.clear();
    r.keys("<C-f>needle<Down><Down>");
    assert_eq!(r.open_files(), ["a.txt"], "no buffer");
    assert_eq!((r.history.clone(), history(&r)), (h, saved), "nothing in the history");
    assert!(r.jobs.contains(&"preview"));
    // no file is asked for, watched, held or snapshotted: only the search, the reads and the
    // highlighter's forgetting of the previews that were left ran
    let other = r
        .log
        .iter()
        .filter(|e| !matches!(e, SimEffect::Grep(_) | SimEffect::Close(_) | SimEffect::Spin(_)));
    assert_eq!(other.count(), 0, "{:?}", r.log);
}

#[test]
fn grep_4_a_late_preview_for_another_file_is_dropped() {
    let mut r = grep_rig("g4-late", &FILES, &[]);
    r.keys("<C-f>needle");
    assert!(r.pane().iter().any(|l| l.contains("alpha needle")));
    let pv = |p: &str, text: &str| {
        SimEvent::Preview(Pv { path: p.into(), file: Ok(Some(text.as_bytes().to_vec())) })
    };
    r.send(pv("c.txt", "nothing here\n"));
    let pane = r.pane();
    assert!(
        pane.iter().any(|l| l.contains("alpha needle")),
        "the selection is still in a.txt: {pane:?}"
    );
    assert!(pane.iter().all(|l| !l.contains("nothing here")));
    // one for the file under the selection is taken in
    r.send(pv("a.txt", "alpha needle\nNEW\n"));
    assert!(r.pane().iter().any(|l| l.contains("NEW")));
}

#[test]
fn grep_4_enter_opens_the_file_at_the_line_and_the_column_and_notes_the_history() {
    let mut r = grep_rig("g4-enter", &FILES, &["c.txt"]);
    r.keys("<C-f>needle<Down><CR>");
    assert_eq!(r.open_files(), ["c.txt", "sub/b.txt"]);
    assert_eq!(r.sim.cur, 1);
    assert_eq!(r.status(), "sub/b.txt  2:1  NORMAL  2/2");
    let h = r.history.clone().unwrap();
    assert!(h.likelihood(&format!("{}/sub/b.txt", r.cwd.display()), NOW) > 0.0);
    assert!(!r.sim.picker_open() && !r.sim.panel.open, "the picker is closed, the panel as it was");
    // the tree shows the file in its place
    assert!(r.sim.tree.row_of(std::path::Path::new("sub/b.txt")).is_some());
    r.keys("<C-n>");
    let tree = r.panel();
    assert!(
        tree.iter().any(|l| l.contains("▾ sub/")) && tree.iter().any(|l| l.contains("b.txt")),
        "{tree:?}"
    );
    // the column of the first match: `alpha needle` has it at 6, and the cursor lands on it
    r.keys("<Esc><C-f>a nee<CR>");
    assert_eq!(r.status(), "a.txt  1:5  NORMAL  3/3");
    assert_eq!(r.app().buf.ch(r.app().buf.cursor), 'a');
}

#[test]
fn grep_4_enter_on_a_file_that_is_open_shows_it_and_moves_its_cursor() {
    let mut r = grep_rig("g4-enter-open", &FILES, &["sub/b.txt"]);
    r.keys(":e a.txt\n");
    assert_eq!(r.sim.cur, 1);
    r.keys("<C-f>Needle<CR>");
    assert_eq!(r.sim.cur, 0, "the open file is shown");
    assert_eq!(r.open_files(), ["sub/b.txt", "a.txt"], "and not opened again");
    assert_eq!(r.status(), "sub/b.txt  2:18  NORMAL  1/2");
    // an App that is not in Normal mode keeps its cursor: the file is shown, and that is all
    r.keys("<Tab>i");
    r.keys("<C-f>alpha<CR>");
    assert_eq!(r.sim.cur, 1);
    assert!(
        r.status().contains("INSERT") && r.status().starts_with("a.txt  1:1"),
        "{}",
        r.status()
    );
}

#[test]
fn grep_4_esc_puts_back_the_file_the_position_and_the_tree() {
    let mut r = grep_rig("g4-esc", &FILES, &[]);
    r.keys("lj"); // sub/ opened, the cursor on b.txt
    let (panel, selected, status) = (r.panel(), r.selected(), r.status().to_string());
    assert!(panel.iter().any(|l| l.contains("b.txt")), "{panel:?}");
    r.keys("<C-f>needle<Down>");
    r.keys("<Esc>");
    assert_eq!((r.panel(), r.selected(), r.status().to_string()), (panel, selected, status));
    assert!(r.sim.panel.open && r.sim.panel.focus, "the focus was in the panel");
    assert!(r.open_files().is_empty());
    // with a file open, the cursor moved, and the panel open without the focus
    let mut r = grep_rig("g4-esc2", &FILES, &["a.txt"]);
    r.keys("<C-n><Esc>lll");
    let (text, status) = (r.text(), r.status().to_string());
    assert!(r.sim.panel.open && !r.sim.panel.focus);
    r.keys("<C-f>needle<Down><Esc>");
    assert!(r.sim.panel.open && !r.sim.panel.focus);
    assert_eq!((r.text(), r.status().to_string()), (text, status));
    assert_eq!(r.open_files(), ["a.txt"]);
}

#[test]
fn grep_5_a_batch_of_an_old_generation_is_dropped() {
    let mut r = grep_rig("g5-generation", &FILES, &[]);
    r.keys("<C-f>needle");
    let list = r.list();
    let line = Line { no: 1, col: 0, text: "needle".into(), marks: vec![0..6] };
    let stale = Group { path: "zzz.txt".into(), lines: vec![line] };
    for generation in [0, 1, 2] {
        let groups = vec![stale.clone()];
        r.send(SimEvent::Grep(Batch { generation, groups, done: true, capped: false }));
    }
    assert_eq!(r.list(), list);
    assert_eq!(r.count(), "1 / 2");
    // and after the search has ended
    r.keys("<Esc>");
    let float = r.float();
    r.send(SimEvent::Grep(Batch {
        generation: 99,
        groups: vec![stale],
        done: true,
        capped: false,
    }));
    assert_eq!(r.float(), float);
}

#[test]
fn grep_5_ending_the_search_asks_the_thread_to_stop() {
    let mut r = grep_rig("g5-stop", &FILES, &[]);
    r.hold_grep = true;
    r.keys("<C-f>needle");
    let generation = held(&r).generation;
    r.grep_held.clear();
    r.keys("<Esc>");
    assert_eq!(r.grep_held.len(), 1);
    assert!(r.grep_held[0].query.is_empty(), "an empty query is the stop");
    assert!(r.grep_held[0].generation > generation, "and it is newer than every search");
}

#[test]
fn grep_5_a_search_names_the_history_first_and_shares_the_list() {
    let files = ["a.txt=hit\n", "b.txt=hit\n", "c.txt=hit\n"];
    let mut r = rig_env("g5-first", &files, &[], (80, 24), |env| {
        let root = env.cwd.display().to_string();
        env.history.record(&format!("{root}/c.txt"), NOW - 100);
        env.history.record(&format!("{root}/gone.txt"), NOW - 10);
        env.history.record("/elsewhere/b.txt", NOW - 10);
    });
    r.hold_grep = true;
    r.keys("<C-f>h");
    let req = held(&r);
    assert_eq!(req.first, ["c.txt"], "files of the history that are in the list, likeliest first");
    assert_eq!(req.files.as_slice(), ["a.txt", "b.txt", "c.txt"]);
    assert_eq!(req.root, r.sim.root);
    assert_eq!(req.query, "h");
}

#[test]
fn grep_5_more_than_a_thousand_lines_say_so() {
    let text: String = "a hit\n".repeat(1500);
    let mut r = grep_rig("g5-cap", &[&format!("big.txt={text}"), "z.txt=hit\n"], &[]);
    r.keys("<C-f>hit");
    assert_eq!(r.count(), "1 / 1,000+");
    assert_eq!(r.list()[0], "▎big.txt:1  a hit");
    r.keys(&"<PageDown>".repeat(300));
    assert_eq!(r.count(), "1,000 / 1,000+");
}

#[test]
fn grep_6_the_count_says_what_the_search_has_found() {
    let mut r = grep_rig("g6-status", &["a.txt=one hit\n", "b.txt=hit hit\nhit\n"], &[]);
    r.hold_grep = true;
    r.keys("<C-f>");
    assert_eq!(r.count(), "0 / 0", "an empty query shows nothing more");
    r.keys("hit");
    assert!(r.count().ends_with('⠋'), "searching: {}", r.count());
    r.deliver_grep();
    assert_eq!(r.count(), "1 / 3");
    r.hold_grep = false;
    r.keys("<BS><BS><BS>one");
    assert_eq!(r.count(), "1 / 1");
    r.keys("x");
    assert_eq!(r.count(), "0 / 0");
    assert_eq!(r.list(), [" no matches"]);
}

#[test]
fn grep_6_a_long_query_is_cut_from_the_left_and_the_cursor_stays_at_its_end() {
    let mut r = grep_rig("g6-long", &["a.txt=hit\n"], &[]);
    r.keys("<C-f>");
    r.keys(&"h".repeat(70));
    let w = r.win().unwrap();
    assert!(r.query().starts_with('…'), "{}", r.query());
    assert!(width(&r.query()) <= usize::from(w.prompt.w));
    assert_eq!(r.screen.cursor.1, w.prompt.y);
    assert!(r.screen.cursor.0 < w.prompt.x + w.prompt.w);
    assert_eq!(r.count(), "0 / 0", "and the count stays");
}

#[test]
fn grep_6_the_pane_s_title_is_the_path_and_line_made_to_fit() {
    let deep = "some/deeply/nested/directory/with/many/parts/file.txt";
    let mut r = grep_rig("g6-path", &[&format!("{deep}=hit here\n"), "a.txt=hit\n"], &[]);
    r.keys("<C-f>hit<Down>");
    let title = r.float().into_iter().find(|l| l.contains("file.txt:1")).expect("the pane's title");
    assert!(title.contains("…/"), "shortened by si's rule: {title}");
    assert!(width(&title) <= usize::from(r.win().unwrap().w), "{title}");
    r.keys("<Up>");
    assert!(
        r.float().iter().any(|l| l.contains("a.txt:1") && l.contains('─')),
        "the title follows"
    );
    // a narrow screen: the picker still fits
    let mut r = grep_rig("g6-narrow", &["a.txt=hit\n"], &[]);
    r.send(SimEvent::Term(Event::Resize(40, 12)));
    r.keys("<C-f>hit<Down>");
    assert!(r.float().iter().all(|l| width(l) <= 40), "{:?}", r.float());
    assert_eq!(r.count(), "1 / 1");
}

/// Neither the rows of the screen nor the bytes the painter writes for it hold anything that is
/// not sim's own (see `foreign`).
fn assert_safe(r: &Rig, what: &str) {
    let all = r.screen.rows.iter().map(|r| r.text.as_str()).collect::<String>();
    assert!(!all.chars().any(|c| c.is_control() || oracle::must_hide(c as u32)), "{what}: {all:?}");
    let mut p = Painter::new(Vec::new());
    p.draw(r.shot.clone()).unwrap();
    assert_eq!(foreign(&p.out), Vec::<String>::new(), "{what}");
}

#[test]
fn grep_7_names_and_contents_with_control_chars_reach_the_terminal_only_as_safe_text() {
    let evil = "ev\u{1b}[2J\u{7}x.txt";
    let content = "needle \u{1b}[31mred\u{1b}]52;c;AAAA\u{7} \u{202e}rtl \u{85}c1 \u{200b}z\tt\n\u{1}needle\u{2}\n";
    let files = [format!("{evil}={content}"), "dir\u{1b}[1mx/ok.txt=needle ok\n".to_string()];
    let mut r = grep_rig("g7-hostile", &[&files[0], &files[1]], &[]);
    r.keys("<C-f>needle");
    assert_eq!(r.count(), "1 / 3");
    assert_safe(&r, "results");
    assert!(r.list()[0].starts_with("▎dir"), "{:?}", r.list());
    assert!(r.pane().join("\n").contains("needle ok"));
    r.keys("<Down>");
    assert_safe(&r, "second result");
    let pane = r.pane().join("\n");
    assert!(
        pane.contains("^[[31m") && pane.contains("<U+202E>"),
        "control chars are tokens: {pane}"
    );
    r.keys("<Down>");
    assert_safe(&r, "third result");
    r.keys("<Up><Up>");
    assert_safe(&r, "browsing back");
    // and the query itself: typed text is cut and replaced like the rest
    r.send(SimEvent::Term(Event::Key(sim_core::core::app::Key::Char('\u{202e}'))));
    assert_safe(&r, "a query with a bidirectional control");
}

#[test]
fn grep_7_hidden_characters_in_names_and_lines_are_shown_in_the_results_and_the_preview() {
    // ASCII smuggling in a file and its name, GlassWorm's selectors, a filler and a ZWJ that joins
    // nothing: the file row, the matching line, the preview and the status line show their forms
    // (SAFE-10), and the match is bold where it is.
    let content = "needle \u{e0049}\u{e0047} \u{fe0f}x\nsecond needle\u{200d}\u{3164}\n";
    let files = [format!("n\u{e0049}.txt={content}"), "plain.txt=needle\n".to_string()];
    let mut r = grep_rig("g7-tags", &[&files[0], &files[1]], &[]);
    r.send(SimEvent::Term(Event::Resize(120, 20)));
    r.keys("<C-f>needle");
    assert_safe(&r, "results");
    let list = lit(&r);
    assert!(
        list.iter()
            .any(|l| l.starts_with("▎n<U+E0049>.txt:1  *needle* <U+E0049><U+E0047> <U+FE0F>x")
                || l.starts_with("n<U+E0049>.txt:1  *needle* <U+E0049><U+E0047> <U+FE0F>x")),
        "{list:?}"
    );
    assert!(list.iter().any(|l| l.contains("second *needle*<U+200D><U+3164>")), "{list:?}");
    r.keys("<Down>");
    assert_safe(&r, "the second result");
    r.keys("<Down>");
    assert_safe(&r, "the third result");
}

#[test]
fn grep_7_a_search_writes_nothing() {
    let mut r = grep_rig("g7-write", &FILES, &["a.txt"]);
    let snapshot = |r: &Rig| {
        let at = |f: &str| {
            let m = std::fs::metadata(r.cwd.join(f)).unwrap();
            (std::fs::read(r.cwd.join(f)).unwrap(), m.modified().unwrap())
        };
        ["a.txt", "sub/b.txt", "c.txt"].map(at)
    };
    let before = snapshot(&r);
    r.log.clear();
    r.keys("<C-f>needle<Down><Esc><C-f>alpha<CR>");
    assert_eq!(snapshot(&r), before);
    let writes = r
        .log
        .iter()
        .filter(|e| matches!(e, SimEffect::App(_, sim_core::core::app::Effect::Write { .. })));
    assert_eq!(writes.count(), 0, "no save was asked for");
}
