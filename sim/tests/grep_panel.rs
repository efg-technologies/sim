//! GREP-1, GREP-3, GREP-4, GREP-5, GREP-6, GREP-7: the content search driven through sim's state
//! (`Sim::update`), with the shell's search and preview played by the test at once.
#![allow(clippy::unwrap_used, clippy::single_range_in_vec_init)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::Event;
use si::core::view::{Shape, Style, width as cols};
use sim::grep::{Batch, Group, Line};
use sim::paint::{Look, Painter};
use sim::sim::{Pv, SimEffect, SimEvent};

/// The panel's rows with their bold parts in `*…*`.
fn lit(r: &Rig) -> Vec<String> {
    let pw = usize::from(r.sim.widths().0);
    let mut out = vec![];
    for i in 0..r.panel().len() {
        let cs: Vec<char> = r.screen.rows[i].text.chars().collect();
        let bold = |col: usize| {
            let on = |(rg, l): &(std::ops::Range<u16>, Look)| {
                *l == Look::Bold && rg.contains(&(col as u16))
            };
            r.shot.looks[i].iter().any(on)
        };
        let (mut text, mut col, mut on) = (String::new(), 0, false);
        for (k, &c) in cs.iter().enumerate() {
            if col >= pw {
                break;
            }
            if bold(col) != on {
                text.push('*');
                on = !on;
            }
            text.push(c);
            col += cols(&cs, k, col);
        }
        text.extend(on.then_some('*'));
        out.push(text.trim_end().to_string());
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
fn grep_1_ctrl_f_from_a_file_opens_the_panel_and_types_a_query() {
    let mut r = grep_rig("g1-open", &FILES, &["a.txt"]);
    assert!(!r.sim.panel.open);
    r.keys("<C-f>");
    assert!(r.sim.panel.open && r.sim.panel.focus);
    assert_eq!(r.status(), "/");
    assert_eq!(r.screen.cursor, (1, 23), "the cursor is at the end of the query");
    r.keys("needle");
    assert_eq!(r.status(), "/needle  2 matches in 2 files");
    assert_eq!(r.screen.cursor, (7, 23));
    assert_eq!(
        r.panel(),
        ["a.txt  1", " 1  alpha needle", "sub/b.txt  1", " 2  needle again and Needl"]
    );
    assert!(r.jobs.contains(&"grep"));
    // the results follow the keys: one more char narrows, Backspace widens
    r.keys("s");
    assert_eq!(r.status(), "/needles  no matches");
    assert!(r.panel().is_empty(), "{:?}", r.panel());
    r.keys("<BS>");
    assert_eq!(r.status(), "/needle  2 matches in 2 files");
}

#[test]
fn grep_1_ctrl_f_works_in_insert_mode_and_never_types() {
    let mut r = grep_rig("g1-insert", &FILES, &["a.txt"]);
    let text = r.text();
    r.keys("i");
    assert!(r.status().contains("INSERT"), "{}", r.status());
    r.keys("<C-f>needle");
    assert_eq!(r.status(), "/needle  2 matches in 2 files");
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
    assert!(!r.sim.panel.open, "the panel was closed before");
    assert_eq!(r.status(), before);
    r.keys("<C-f>n<BS>");
    assert_eq!(r.status(), "/");
    r.keys("<BS>");
    assert!(!r.sim.panel.open);
    assert_eq!(r.status(), before);
}

#[test]
fn grep_1_ctrl_f_again_goes_back_to_typing_and_keeps_the_query() {
    let mut r = grep_rig("g1-again", &FILES, &["a.txt"]);
    r.keys("<C-f>need<Down>");
    assert_eq!(r.screen.shape, Shape::Block, "the results are browsed");
    r.keys("<C-f>");
    assert_eq!(r.screen.shape, Shape::Bar, "typing again");
    r.keys("le");
    assert_eq!(r.status(), "/needle  2 matches in 2 files");
    r.keys("<Down>/");
    assert_eq!(r.screen.shape, Shape::Bar, "`/` goes back to typing as well");
    r.keys("s");
    assert!(r.status().starts_with("/needles"), "{}", r.status());
}

#[test]
fn grep_1_ctrl_f_ends_a_name_search_and_ctrl_p_and_ctrl_n_end_a_content_search() {
    let mut r = grep_rig("g1-keys", &FILES, &[]);
    r.keys("/c.t");
    assert!(r.status().starts_with("/c.t  "));
    r.keys("<C-f>");
    assert_eq!(r.status(), "/", "a new, empty query");
    r.keys("needle");
    assert!(r.status().ends_with("in 2 files"), "{}", r.status());
    assert_eq!(r.panel()[0], "a.txt  1", "the results are in the panel");
    r.keys("<C-p>");
    assert_eq!(r.status(), "/", "FIND-4's search");
    assert!(r.panel().iter().any(|l| l.contains("sub/")), "the tree is back: {:?}", r.panel());
    r.keys("<C-f>needle<C-n>");
    assert!(!r.sim.panel.open, "Ctrl-n closes the panel");
    r.keys("<C-n>");
    assert!(r.panel().iter().any(|l| l.contains("sub/")), "and the tree is what it was");
}

#[test]
fn grep_1_tab_paste_and_the_mouse_do_nothing_while_it_is_on() {
    let mut r = grep_rig("g1-inert", &FILES, &["a.txt"]);
    r.keys(":e c.txt\n");
    assert_eq!(r.sim.cur, 1);
    r.keys("<C-f>needle<Down>");
    let (status, panel, sel) = (r.status().to_string(), r.panel(), r.selected());
    r.keys("<Tab>");
    r.send(SimEvent::Term(Event::Paste("zzz".into())));
    r.click(3, 1);
    r.click(40, 3);
    r.wheel(true, 3, 3);
    assert_eq!((r.status(), r.panel(), r.selected()), (status.as_str(), panel, sel));
    assert_eq!(r.sim.cur, 1, "Tab did not switch files");
}

#[test]
fn grep_3_results_are_grouped_by_file_with_the_matches_bold_and_the_numbers_dim() {
    let mut r = grep_rig("g3-looks", &FILES, &[]);
    r.keys("<C-f>needle");
    assert_eq!(
        lit(&r),
        ["a.txt  1", " 1  alpha *needle*", "sub/b.txt  1", " 2  *needle* again and *Needl*"]
    );
    r.keys("<BS><BS><BS><BS><BS><BS>nEEdle");
    assert_eq!(r.status(), "/nEEdle  no matches", "an uppercase letter makes it exact");
    r.keys("<BS><BS><BS><BS><BS><BS>Needle");
    // (the match would be cut off by the panel's width, so the row starts a little before it)
    assert_eq!(lit(&r), ["sub/b.txt  1", " 2  …and *Needle*"]);
    // the line numbers are the secondary style, the file row is plain, the cursor row underlined
    let pw = r.sim.widths().0;
    let sec = |row: usize| {
        let dim = |(rg, s): &(std::ops::Range<u16>, Style)| {
            *s == Style::Secondary && rg.start == 1 && rg.end <= pw
        };
        r.screen.rows[row].spans.iter().any(dim)
    };
    assert!(!sec(0) && sec(1));
    assert_eq!(r.selected().as_deref(), Some(" 2  …and Needle"));
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
    let files: Vec<String> = r.panel().into_iter().filter(|l| l.contains(".txt  ")).collect();
    assert_eq!(files, ["c.txt  1", "d/e.txt  1", "a.txt  1", "b.txt  1"]);
}

#[test]
fn grep_3_batches_are_put_in_place_whatever_order_they_come_in_and_the_cursor_stays() {
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
    assert_eq!(r.status(), "/hit  searching…");
    r.send(batch(vec![group("m.txt", &[1, 2, 3])], false));
    assert_eq!(r.panel(), ["m.txt  3", " 1  hit m.txt1", " 2  hit m.txt2", " 3  hit m.txt3"]);
    assert_eq!(r.status(), "/hit  3 matches in 1 file…", "partial: the count ends with …");
    r.keys("<Down>");
    assert_eq!(r.selected().as_deref(), Some(" 2  hit m.txt2"));
    r.send(batch(vec![group("a.txt", &[7]), group("z.txt", &[9])], true));
    assert_eq!(
        r.panel(),
        [
            "a.txt  1",
            " 7  hit a.txt7",
            "m.txt  3",
            " 1  hit m.txt1",
            " 2  hit m.txt2",
            " 3  hit m.txt3",
            "z.txt  1",
            " 9  hit z.txt9"
        ]
    );
    assert_eq!(r.selected().as_deref(), Some(" 2  hit m.txt2"), "the cursor stays on its line");
    assert_eq!(r.status(), "/hit  5 matches in 3 files  m.txt:2");
}

#[test]
fn grep_3_the_old_results_stay_until_the_first_batch_of_the_new_query_comes() {
    let mut r = grep_rig("g3-stale", &FILES, &[]);
    r.keys("<C-f>needle");
    assert_eq!(r.panel()[0], "a.txt  1");
    r.hold_grep = true;
    r.keys("<BS>");
    assert_eq!(r.status(), "/needl  searching…");
    assert_eq!(r.panel()[0], "a.txt  1", "nothing blinked");
    r.keys("<CR>");
    assert!(r.open_files().is_empty(), "Enter does nothing while the results are the old query's");
    r.deliver_grep();
    assert_eq!(r.status(), "/needl  2 matches in 2 files");
    // and a query with no match: the end replaces the old results by nothing
    r.keys("zz");
    assert_eq!(r.panel()[0], "a.txt  1");
    r.deliver_grep();
    assert!(r.panel().is_empty());
    assert_eq!(r.status(), "/needlzz  no matches");
}

#[test]
fn grep_4_j_k_and_the_arrows_move_over_matching_lines_n_and_big_n_wrap() {
    let files = ["a.txt=hit 1\nx\nhit 3\n", "b.txt=hit b\n"];
    let mut r = grep_rig("g4-move", &files, &[]);
    r.keys("<C-f>hit");
    assert_eq!(r.selected().as_deref(), Some(" 1  hit 1"), "the first line is under the cursor");
    r.keys("<Down>");
    assert_eq!(r.selected().as_deref(), Some(" 3  hit 3"), "file rows are skipped");
    r.keys("j");
    assert_eq!(r.selected().as_deref(), Some(" 1  hit b"));
    r.keys("j<Down>");
    assert_eq!(r.selected().as_deref(), Some(" 1  hit b"), "j stops at the last line");
    r.keys("n");
    assert_eq!(r.selected().as_deref(), Some(" 1  hit 1"), "n wraps round");
    r.keys("N");
    assert_eq!(r.selected().as_deref(), Some(" 1  hit b"), "N wraps back");
    r.keys("kk");
    assert_eq!(r.selected().as_deref(), Some(" 1  hit 1"));
    r.keys("k<Up>");
    assert_eq!(r.selected().as_deref(), Some(" 1  hit 1"), "k stops at the first");
    r.keys("G");
    assert_eq!(r.selected().as_deref(), Some(" 1  hit b"));
    r.keys("gg");
    assert_eq!(r.selected().as_deref(), Some(" 1  hit 1"));
    // while the query is typed, they are chars of it
    r.keys("/jk");
    assert!(r.status().starts_with("/hitjk"), "{}", r.status());
}

#[test]
fn grep_4_the_panel_scrolls_to_keep_the_cursor_row_and_its_file_row_in_view() {
    let text: String = (1..=60).map(|i| format!("hit {i}\n")).collect();
    let mut r = grep_rig("g4-scroll", &[&format!("a.txt={text}"), "b.txt=hit b\n"], &[]);
    r.keys("<C-f>hit");
    assert_eq!(r.panel()[0], "a.txt  60");
    r.keys("<Down>G");
    assert_eq!(r.selected().as_deref(), Some(" 1  hit b"));
    assert_eq!(r.panel().len(), 23, "the panel is full, scrolled");
    assert_eq!(r.panel()[21..], ["b.txt  1", " 1  hit b"], "the file row is above its line");
    r.keys("gg");
    assert_eq!(r.panel()[0], "a.txt  60", "back at the top, the file row is there");
    r.keys("<PageDown>");
    assert!(r.selected().is_some_and(|s| s.contains("hit 2")), "{:?}", r.selected());
}

#[test]
fn grep_4_the_editor_previews_the_file_under_the_cursor_with_the_line_in_the_middle() {
    let long: String = (1..=100)
        .map(|i| if i == 60 { "the needle is here\n".to_string() } else { format!("line {i}\n") })
        .collect();
    let mut r =
        grep_rig("g4-preview", &["a.txt=one\ntwo\n", &format!("big.txt={long}")], &["a.txt"]);
    let text = r.text();
    r.keys("<C-f>needle");
    let ed = r.editor();
    let at = ed
        .iter()
        .position(|l| l.contains("the needle is here"))
        .unwrap_or_else(|| panic!("{ed:?}"));
    assert!((9..=13).contains(&at), "the line is in the middle of 23 rows, not at row {at}");
    assert!(ed[at].contains(" 60 "), "with its number: {}", ed[at]);
    assert!(ed.iter().any(|l| l.contains("line 55")) && ed.iter().all(|l| !l.contains("line 1 ")));
    // the match is bold, on the row of the pane that starts at column pw + 1
    let pw = r.sim.widths().0;
    let col = r.screen.rows[at].text.find("needle").unwrap() as u16;
    let bold = |(rg, l): &(std::ops::Range<u16>, Look)| {
        *l == Look::Bold && rg.start == col && rg.end == col + 6
    };
    assert!(col > pw && r.shot.looks[at].iter().any(bold), "{col}: {:?}", r.shot.looks[at]);
    assert_eq!(r.shot.looks[at].len(), 1, "and nothing else on the row");
    // the file that was open is not in the pane, and is not touched
    assert!(ed.iter().all(|l| !l.contains("one")));
    assert_eq!(r.text(), text);
}

#[test]
fn grep_4_the_preview_follows_the_cursor_to_another_file_and_line() {
    let mut r = grep_rig("g4-follow", &FILES, &[]);
    r.keys("<C-f>needle");
    assert!(r.editor().iter().any(|l| l.contains("alpha needle")));
    r.keys("<Down>");
    let ed = r.editor();
    assert!(ed.iter().any(|l| l.contains("needle again and Needle")), "{ed:?}");
    assert!(ed.iter().all(|l| !l.contains("alpha")));
    assert!(ed.iter().any(|l| l.contains("gamma")), "the line before it is there too");
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
    // no file is asked for, watched, held or snapshotted: only the search and the read ran
    assert!(r.log.is_empty(), "{:?}", r.log);
}

#[test]
fn grep_4_a_late_preview_for_another_file_is_dropped() {
    let mut r = grep_rig("g4-late", &FILES, &[]);
    r.keys("<C-f>needle");
    assert!(r.editor().iter().any(|l| l.contains("alpha needle")));
    let pv = |p: &str, text: &str| {
        SimEvent::Preview(Pv { path: p.into(), file: Ok(Some(text.as_bytes().to_vec())) })
    };
    r.send(pv("c.txt", "nothing here\n"));
    let ed = r.editor();
    assert!(ed.iter().any(|l| l.contains("alpha needle")), "the cursor is still in a.txt: {ed:?}");
    assert!(ed.iter().all(|l| !l.contains("nothing here")));
    // one for the file under the cursor is taken in
    r.send(pv("a.txt", "alpha needle\nNEW\n"));
    assert!(r.editor().iter().any(|l| l.contains("NEW")));
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
    assert!(r.sim.panel.open && !r.sim.panel.focus, "the panel stays, the focus is in the text");
    let tree = r.panel();
    assert!(
        tree.iter().any(|l| l.contains("▾ sub/")) && tree.iter().any(|l| l.contains("b.txt")),
        "{tree:?}"
    );
    // the column of the first match: `alpha needle` has it at 6, and the cursor lands on it
    r.keys("<C-f>a nee<CR>");
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
    assert_ne!(r.panel(), panel);
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
    let panel = r.panel();
    let line = Line { no: 1, col: 0, text: "needle".into(), marks: vec![0..6] };
    let stale = Group { path: "zzz.txt".into(), lines: vec![line] };
    for generation in [0, 1, 2] {
        let groups = vec![stale.clone()];
        r.send(SimEvent::Grep(Batch { generation, groups, done: true, capped: false }));
    }
    assert_eq!(r.panel(), panel);
    assert_eq!(r.status(), "/needle  2 matches in 2 files");
    // and after the search has ended
    r.keys("<Esc>");
    let panel = r.panel();
    r.send(SimEvent::Grep(Batch {
        generation: 99,
        groups: vec![stale],
        done: true,
        capped: false,
    }));
    assert_eq!(r.panel(), panel);
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
    assert_eq!(r.status(), "/hit  1000+ matches in 1 file");
    assert_eq!(r.panel()[0], "big.txt  1000");
    r.keys("<Down>G");
    assert_eq!(r.selected().as_deref(), Some(" 1000  a hit"));
}

#[test]
fn grep_6_the_status_line_says_what_the_search_has_found() {
    let mut r = grep_rig("g6-status", &["a.txt=one hit\n", "b.txt=hit hit\nhit\n"], &[]);
    r.hold_grep = true;
    r.keys("<C-f>");
    assert_eq!(r.status(), "/", "an empty query shows nothing more");
    r.keys("hit");
    assert_eq!(r.status(), "/hit  searching…");
    r.deliver_grep();
    assert_eq!(r.status(), "/hit  3 matches in 2 files");
    r.hold_grep = false;
    r.keys("<BS><BS><BS>one");
    assert_eq!(r.status(), "/one  1 match in 1 file");
    r.keys("x");
    assert_eq!(r.status(), "/onex  no matches");
}

#[test]
fn grep_6_a_long_query_is_cut_from_the_left_and_the_count_stays() {
    let mut r = grep_rig("g6-long", &["a.txt=hit\n"], &[]);
    r.keys("<C-f>");
    r.keys(&"h".repeat(70));
    assert!(r.status().starts_with('…') && r.status().ends_with("  no matches"), "{}", r.status());
    assert!(width(r.status()) <= 80);
    assert_eq!(r.screen.cursor.1, 23);
    assert!(r.screen.cursor.0 < 80);
    // with matches: the count is kept, the query cut
    let mut r = grep_rig("g6-long2", &[&format!("a.txt={}\n", "h".repeat(70))], &[]);
    r.keys("<C-f>");
    r.keys(&"h".repeat(70));
    let end = "  1 match in 1 file";
    assert!(r.status().starts_with('…') && r.status().ends_with(end), "{}", r.status());
    assert!(width(r.status()) <= 80);
}

#[test]
fn grep_6_browsing_adds_the_path_and_line_made_to_fit() {
    let deep = "some/deeply/nested/directory/with/many/parts/file.txt";
    let mut r = grep_rig("g6-path", &[&format!("{deep}=hit here\n"), "a.txt=hit\n"], &[]);
    r.keys("<C-f>hit<Down>");
    assert!(r.status().starts_with("/hit  2 matches in 2 files  "), "{}", r.status());
    assert!(r.status().ends_with("file.txt:1"), "{}", r.status());
    assert!(r.status().contains("…/"), "shortened by si's rule: {}", r.status());
    assert!(width(r.status()) <= 80, "{}", r.status());
    r.keys("<Up>");
    assert!(r.status().ends_with("  a.txt:1"), "{}", r.status());
    // a narrow screen: the path gives way first
    let mut r = grep_rig("g6-narrow", &["a.txt=hit\n"], &[]);
    r.send(SimEvent::Term(Event::Resize(40, 12)));
    r.keys("<C-f>hit<Down>");
    assert!(width(r.status()) <= 40, "{}", r.status());
    assert!(r.status().starts_with("/hit  1 match in 1 file"), "{}", r.status());
}

/// Neither the rows of the screen nor the bytes the painter writes for it hold anything that is
/// not sim's own (see `foreign`).
fn assert_safe(r: &Rig, what: &str) {
    let all = r.screen.rows.iter().map(|r| r.text.as_str()).collect::<String>();
    assert!(!all.chars().any(|c| c.is_control() || BIDI.contains(c)), "{what}: {all:?}");
    let mut p = Painter::new(Vec::new());
    p.draw(r.shot.clone()).unwrap();
    assert_eq!(foreign(&p.out), Vec::<String>::new(), "{what}");
}

const BIDI: &str = "\u{61c}\u{200b}\u{200e}\u{200f}\u{202a}\u{202e}\u{2066}\u{2069}\u{feff}";

#[test]
fn grep_7_names_and_contents_with_control_chars_reach_the_terminal_only_as_safe_text() {
    let evil = "ev\u{1b}[2J\u{7}x.txt";
    let content = "needle \u{1b}[31mred\u{1b}]52;c;AAAA\u{7} \u{202e}rtl \u{85}c1 \u{200b}z\tt\n\u{1}needle\u{2}\n";
    let files = [format!("{evil}={content}"), "dir\u{1b}[1mx/ok.txt=needle ok\n".to_string()];
    let mut r = grep_rig("g7-hostile", &[&files[0], &files[1]], &[]);
    r.keys("<C-f>needle");
    assert_eq!(r.status(), "/needle  3 matches in 2 files");
    assert_safe(&r, "results");
    assert!(r.panel()[0].starts_with("dir"), "{:?}", r.panel());
    assert!(r.editor().join("\n").contains("needle ok"));
    r.keys("<Down>");
    assert_safe(&r, "second result");
    let ed = r.editor().join("\n");
    assert!(ed.contains("^[[31m") && ed.contains("<U+202E>"), "control chars are tokens: {ed}");
    r.keys("<Down>");
    assert_safe(&r, "third result");
    r.keys("<Up><Up>");
    assert_safe(&r, "browsing back");
    // and the query itself: typed text is cut and replaced like the rest
    r.keys("/");
    r.send(SimEvent::Term(Event::Key(si::core::app::Key::Char('\u{202e}'))));
    assert_safe(&r, "a query with a bidirectional control");
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
        .filter(|e| matches!(e, SimEffect::App(_, si::core::app::Effect::Write { .. })));
    assert_eq!(writes.count(), 0, "no save was asked for");
}
