//! FIND-1, FIND-2, FIND-3, FIND-4: the search in the panel, driven through sim's state, and the
//! list of files it searches.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use si::core::app::{Event, Key};
use si::core::view::Style;
use sim::io;
use sim::paint::Look;
use sim::sim::{Env, Sim, SimEffect, SimEvent, start};
use std::path::PathBuf;

const FILES: [&str; 8] = [
    "src/main.rs",
    "src/lib.rs",
    "src/ui/panel.rs",
    "docs/guide.md",
    "docs/notes/main.md",
    "README.md",
    "Cargo.toml",
    "build.rs",
];

/// The panel's rows with `*` before those that are bold, `-` before those that are dimmed.
fn lit(r: &Rig) -> Vec<String> {
    let (pw, _) = r.sim.widths();
    let rows = r.panel();
    let bold = |i: usize| r.shot.looks[i].iter().any(|(s, l)| *l == Look::Bold && s.start == 0);
    let dim = |i: usize| {
        r.screen.rows[i]
            .spans
            .iter()
            .any(|(s, st)| *st == Style::Secondary && s.start == 0 && s.end > 0 && s.end <= pw)
    };
    rows.iter()
        .enumerate()
        .map(|(i, t)| {
            format!(
                "{}{t}",
                if bold(i) {
                    "*"
                } else if dim(i) {
                    "-"
                } else {
                    " "
                }
            )
        })
        .collect()
}

#[test]
fn find_1_slash_searches_every_file_and_takes_the_cursor_to_the_best_match() {
    let mut r = rig("f1-best", &FILES, &[]);
    assert_eq!(r.panel(), ["▸ docs/", "▸ src/", "  build.rs", "  Cargo.toml", "  README.md"]);
    r.keys("/");
    assert_eq!(r.status(), "/", "an empty query matches nothing and lights nothing");
    assert!(lit(&r).iter().all(|l| l.starts_with(' ')));
    r.keys("panel");
    assert_eq!(r.status(), "/panel  1 match");
    // Nothing was expanded by hand, yet the file is shown with every directory on its way open.
    assert_eq!(r.selected().as_deref(), Some("      panel.rs"));
    assert_eq!(
        r.panel(),
        [
            "▸ docs/",
            "▾ src/",
            "  ▾ ui/",
            "      panel.rs",
            "    lib.rs",
            "    main.rs",
            "  build.rs",
            "  Cargo.toml",
            "  README.md"
        ]
    );
    assert_eq!(r.screen.cursor, (1 + "panel".len() as u16, 23), "the cursor is in the query");
    // The query goes on as keys come: `ma` matches both main files and the Cargo.toml's `a`...
    r.keys("<BS><BS><BS><BS><BS>ma");
    assert!(r.status().starts_with("/ma  "), "{}", r.status());
}

#[test]
fn find_1_matches_and_their_directories_are_bold_and_the_rest_is_dimmed_and_nothing_moves() {
    let mut r = rig_sized("f1-lit", &FILES, &[], (44, 11));
    let before = r.panel();
    r.keys("/main");
    assert_eq!(r.status(), "/main  2 matches");
    assert_eq!(
        lit(&r),
        [
            "*▸ docs/",
            "*▾ src/",
            "-  ▸ ui/",
            "-    lib.rs",
            "*    main.rs",
            "-  build.rs",
            "-  Cargo.toml",
            "-  README.md"
        ],
        "directories first, then names: the order is the tree's, as always; docs/ holds a match"
    );
    golden("find_1_panel_search", &dump_screen(&r.shot));
    // Ending the search gives back the rows the panel had.
    r.keys("<Esc>");
    assert_eq!(r.panel(), before);
}

#[test]
fn find_1_n_and_shift_n_go_to_the_next_and_the_previous_match_in_the_order_on_screen() {
    let mut r = rig("f1-n", &FILES, &[]);
    r.keys("/main<Down>");
    // Down ended the typing and stepped on: from the best match (src/main.rs) round to the first.
    assert_eq!(r.selected().as_deref(), Some("      main.md"));
    assert_eq!(r.status(), "/main  2 matches");
    r.keys("n");
    assert_eq!(r.selected().as_deref(), Some("    main.rs"));
    r.keys("n");
    assert_eq!(r.selected().as_deref(), Some("      main.md"), "round to the first");
    r.keys("N");
    assert_eq!(r.selected().as_deref(), Some("    main.rs"), "round to the last");
    r.keys("N");
    assert_eq!(r.selected().as_deref(), Some("      main.md"));
    // From a row that is not a match, `n` goes to the next match below it.
    r.keys("k");
    assert_eq!(r.selected().as_deref(), Some("  ▾ notes/"));
    r.keys("n");
    assert_eq!(r.selected().as_deref(), Some("      main.md"));
    // Other keys are the panel's own, while the matches stay lit.
    r.keys("jj");
    assert_eq!(r.selected().as_deref(), Some("▾ src/"));
    assert!(lit(&r).iter().any(|l| l.starts_with('-')));
}

#[test]
fn find_1_enter_opens_the_best_match_at_once_and_the_tree_shows_where_it_is() {
    let mut r = rig("f1-enter", &FILES, &[]);
    r.keys("/panel\n");
    assert_eq!(r.open_files(), ["src/ui/panel.rs"]);
    assert!(!r.sim.panel.focus && r.sim.panel.open, "the focus moved to the text");
    assert!(
        r.panel().contains(&"    • panel.rs".to_string()),
        "directories stay open: {:?}",
        r.panel()
    );
    assert!(lit(&r).iter().all(|l| l.starts_with(' ')), "and the search is over");
    // With nothing found Enter does nothing, and the query can be edited.
    r.keys("<C-p>zzzz\n");
    assert_eq!(r.open_files(), ["src/ui/panel.rs"]);
    assert_eq!(r.status(), "/zzzz  no match");
    r.keys("<BS><BS><BS><BS>");
    assert_eq!(r.status(), "/");
}

#[test]
fn find_1_enter_opens_the_best_match_even_before_its_directory_has_been_read() {
    let mut r = rig("f1-late", &FILES, &[]);
    r.defer = true; // the listings of directories arrive late
    r.keys("/panel");
    assert_eq!(r.selected().as_deref(), Some("▸ docs/"), "the cursor has not moved yet");
    r.keys("\n");
    assert_eq!(r.open_files(), ["src/ui/panel.rs"], "Enter opens the match, not the row");
    r.deliver();
    r.deliver();
    assert!(r.panel().contains(&"    • panel.rs".to_string()), "{:?}", r.panel());
}

#[test]
fn find_1_a_directory_that_is_read_later_is_expanded_when_its_listing_arrives() {
    let mut r = rig("f1-reveal", &FILES, &[]);
    r.defer = true;
    r.keys("/panel");
    r.deliver(); // src
    r.deliver(); // src/ui
    assert_eq!(r.selected().as_deref(), Some("      panel.rs"));
}

#[test]
fn find_1_esc_ends_the_search_and_restores_the_expansion_and_the_cursor() {
    let mut r = rig("f1-esc", &FILES, &[]);
    r.keys("jl"); // src/ opened by hand
    let before = (r.panel(), r.selected());
    assert_eq!(before.1.as_deref(), Some("▾ src/"));
    r.keys("/panel");
    assert!(r.panel().contains(&"  ▾ ui/".to_string()) && r.panel().len() > before.0.len());
    r.keys("<Esc>");
    assert_eq!((r.panel(), r.selected()), before, "the expansion and the cursor are back");
    assert!(r.sim.panel.focus, "the focus stays in the panel; a second Esc leaves it");
    r.keys("<Esc>");
    assert!(!r.sim.panel.focus);
    // The same after `n`: the matches stay lit until Esc.
    let mut r = rig("f1-esc2", &FILES, &[]);
    r.keys("/main<Down><Esc>");
    assert!(lit(&r).iter().all(|l| l.starts_with(' ')));
    assert_eq!(r.panel(), ["▸ docs/", "▸ src/", "  build.rs", "  Cargo.toml", "  README.md"]);
    assert_eq!(r.selected().as_deref(), Some("▸ docs/"));
}

#[test]
fn find_1_backspace_edits_the_query_and_ends_the_search_when_there_is_nothing_left() {
    let mut r = rig("f1-bs", &FILES, &[]);
    r.keys("/mainx");
    assert_eq!(r.status(), "/mainx  no match");
    assert!(lit(&r).iter().all(|l| l.starts_with('-')), "everything is dimmed: nothing matches");
    r.keys("<BS>");
    assert_eq!(r.status(), "/main  2 matches");
    r.keys("<BS><BS><BS><BS>");
    assert_eq!(r.status(), "/");
    r.keys("<BS>");
    assert_eq!(r.status(), "docs/  TREE", "the search is over");
}

#[test]
fn find_1_a_long_query_keeps_the_count_and_the_cursor_on_the_screen() {
    let name = "q".repeat(100);
    let mut r = rig("f1-long", &[&name], &[]);
    r.keys(&format!("/{name}"));
    let (text, w) = (r.status().to_string(), 80);
    assert!(text.starts_with("…q") && text.ends_with("q  1 match"), "{text}");
    assert_eq!(width(&text), w, "{text}");
    assert!(usize::from(r.screen.cursor.0) < w && r.screen.cursor.1 == 23, "{:?}", r.screen.cursor);
    r.keys("z");
    assert!(r.status().ends_with("z  no match") && width(r.status()) <= w, "{}", r.status());
}

#[test]
fn find_1_the_query_is_text_and_every_key_but_the_few_is_ignored() {
    let mut r = rig("f1-keys", &FILES, &[]);
    r.keys("/njq.<Left><C-d><PageDown>");
    assert_eq!(r.status(), "/njq.  no match");
    r.keys("<Esc>nj");
    assert_eq!(r.selected().as_deref(), Some("▸ src/"), "after Esc, n and j are the panel's");
}

#[test]
fn find_1_smartcase_and_unicode_queries() {
    let mut r = rig("f1-case", &["a/Readme.md", "a/readme.txt", "日本語/メモ.md"], &[]);
    r.keys("/readme");
    assert_eq!(r.status(), "/readme  2 matches");
    r.keys("<Esc>/Readme");
    assert_eq!(r.status(), "/Readme  1 match");
    r.keys("<Esc>/メモ");
    assert_eq!(r.status(), "/メモ  1 match");
}

#[test]
fn find_2_the_likelihood_of_a_file_decides_between_equal_matches() {
    let two = ["a/util.rs", "b/util.rs"];
    let mut r = rig("f2-tie", &two, &[]);
    r.keys("/util");
    assert_eq!(&r.panel()[..3], ["▾ a/", "    util.rs", "▸ b/"], "equal matches: the smaller path");
    // Now b/util.rs was opened a couple of minutes ago.
    let mut r = rig_env("f2-tie2", &two, &[], (80, 24), |e: &mut Env| {
        e.history.record(&e.cwd.join("b/util.rs").to_string_lossy(), NOW - 120);
    });
    r.keys("/util");
    assert_eq!(&r.panel()[..3], ["▸ a/", "▾ b/", "  • util.rs"], "the likelier one");
    // A better match still beats a likelier file: the likelihood is only the tiebreak.
    let mut r = rig_env("f2-tie3", &["a/utility.rs", "b/util.rs"], &[], (80, 24), |e: &mut Env| {
        e.history.record(&e.cwd.join("a/utility.rs").to_string_lossy(), NOW - 120);
    });
    r.keys("/util.");
    assert_eq!(&r.panel()[..3], ["▸ a/", "▾ b/", "    util.rs"]);
}

#[test]
fn find_3_the_list_is_asked_at_the_start_and_when_the_panel_opens_and_never_twice_at_once() {
    let mut r = rig("f3-asks", &["f.txt"], &["f.txt"]);
    assert_eq!(r.jobs.iter().filter(|j| **j == "files").count(), 1, "at the start");
    r.keys("<C-n>");
    assert_eq!(r.jobs.iter().filter(|j| **j == "files").count(), 2, "when the panel opens");
    r.keys("<C-n><C-p>");
    assert_eq!(r.jobs.iter().filter(|j| **j == "files").count(), 3, "and Ctrl-p opens it too");
}

#[test]
fn find_3_the_shell_runs_the_list_never_twice_at_once_and_once_more_after_requests_in_between() {
    use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
    use std::sync::{Arc, mpsc};
    let (runs, (tx, rx)) = (Arc::new(AtomicUsize::new(0)), mpsc::channel());
    let counter = runs.clone();
    let ask = io::worker(tx, move || {
        let n = counter.fetch_add(1, SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(150));
        SimEvent::Files(vec![format!("run{n}")])
    });
    ask.send(()).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50)); // it runs now
    (0..20).for_each(|_| ask.send(()).unwrap());
    let answers: Vec<SimEvent> =
        (0..2).map(|_| rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap()).collect();
    assert!(matches!(&answers[1], SimEvent::Files(f) if f == &["run1"]), "{answers:?}");
    assert!(
        rx.recv_timeout(std::time::Duration::from_millis(400)).is_err(),
        "twenty requests were one run"
    );
    assert_eq!(runs.load(SeqCst), 2);
}

#[test]
fn find_3_sim_asks_for_the_list_at_the_start_and_when_the_panel_opens() {
    let cwd = PathBuf::from("/nonexistent/cwd");
    let s = start(&[], &cwd, |_| false, |_| false).unwrap();
    let env = Env { cwd, size: (80, 24), now: NOW, ..Env::default() };
    let (mut sim, fx) = Sim::new(s, env);
    let count = |fx: &[SimEffect]| fx.iter().filter(|e| matches!(e, SimEffect::Files)).count();
    assert_eq!(count(&fx), 1, "at the start");
    let key = |sim: &mut Sim, k| sim.update(SimEvent::Term(Event::Key(k)));
    assert_eq!(count(&key(&mut sim, Key::Ctrl('n'))), 0, "closes the panel");
    assert_eq!(count(&key(&mut sim, Key::Ctrl('n'))), 1, "opens it");
}

#[test]
fn find_3_the_files_of_a_repository_are_tracked_and_untracked_ones_but_not_ignored_ones() {
    let d = temp_dir("f3-git");
    repo(&d.0, &["src/a.rs", "src/b.rs", "README.md"]);
    make(&d.0, &["src/untracked.rs", "target/junk.rs", "debug.log", "dir/deep/file.txt"]);
    std::fs::write(d.0.join(".gitignore"), "target/\n*.log\n").unwrap();
    let got = io::files(&d.0);
    assert_eq!(
        got,
        [
            ".gitignore",
            "README.md",
            "dir/deep/file.txt",
            "src/a.rs",
            "src/b.rs",
            "src/untracked.rs"
        ]
    );
    // From a subdirectory: relative to it, and only what is below it.
    assert_eq!(io::files(&d.0.join("src")), ["a.rs", "b.rs", "untracked.rs"]);
}

#[test]
fn find_3_outside_git_the_files_come_from_a_walk_that_skips_dot_git_and_links() {
    let d = temp_dir("f3-walk");
    make(&d.0, &["a.txt", "sub/b.txt", "sub/deeper/c.txt", ".hidden", "x/.git/config", "empty/"]);
    std::os::unix::fs::symlink(&d.0, d.0.join("sub/loop")).unwrap();
    std::os::unix::fs::symlink("a.txt", d.0.join("link")).unwrap();
    let got = io::files(&d.0);
    assert_eq!(got, [".hidden", "a.txt", "link", "sub/b.txt", "sub/deeper/c.txt", "sub/loop"]);
}

#[test]
fn find_3_a_list_is_sorted_and_has_no_duplicates_whatever_git_says() {
    let d = temp_dir("f3-sorted");
    repo(&d.0, &["b.rs", "a.rs", "dir/z.rs", "dir/a.rs", "Dir/x.rs"]);
    let got = io::files(&d.0);
    let mut want = got.clone();
    want.sort();
    want.dedup();
    assert_eq!(got, want);
    assert_eq!(got.len(), 5);
}

#[test]
fn find_4_ctrl_p_opens_the_panel_and_starts_the_search_from_anywhere() {
    for (name, keys) in
        [("normal", ""), ("insert", "i"), ("cmdline", ":ab"), ("visual", "v"), ("pending", "d")]
    {
        let mut r = rig(&format!("f4-{name}"), &["x.txt=hi\n", "y/z.txt"], &["x.txt"]);
        assert!(!r.sim.panel.open);
        r.keys(keys);
        r.keys("<C-p>z");
        assert!(r.sim.panel.open && r.sim.panel.focus, "{name}");
        assert_eq!(r.status(), "/z  1 match", "{name}");
        assert_eq!(r.selected().as_deref(), Some("    z.txt"), "{name}");
        assert_eq!(r.text(), "hi", "{name}: the key never reached the text");
    }
}

#[test]
fn find_4_ctrl_p_with_the_panel_open_and_unfocused_focuses_it_and_a_second_one_starts_over() {
    let mut r = rig("f4-open", &["x.txt", "y/z.txt"], &["x.txt"]);
    r.keys("<C-n><Esc>");
    assert!(r.sim.panel.open && !r.sim.panel.focus);
    r.keys("<C-p>z");
    assert!(r.sim.panel.focus);
    assert_eq!(r.status(), "/z  1 match");
    r.keys("<C-p>");
    assert_eq!(r.status(), "/", "a new search");
    r.keys("x\n");
    assert_eq!(r.open_files(), ["x.txt"]);
}

#[test]
fn find_4_ctrl_p_then_enter_is_a_quick_open_in_three_steps() {
    let mut r = rig("f4-quick", &["a.txt", "dir/target.rs=fn t() {}\n", "z.txt"], &["a.txt"]);
    r.keys("<C-p>targ\n");
    assert_eq!(r.open_files(), ["a.txt", "dir/target.rs"]);
    assert_eq!(r.sim.cur, 1);
    assert_eq!(r.status(), "dir/target.rs  1:1  NORMAL  2/2");
    assert!(!r.sim.panel.focus);
}

#[test]
#[ignore = "benchmark: run with --release"]
fn find_3_bench_a_key_of_the_search_in_sim_over_100000_files() {
    release_only();
    let (names, mut r) = big_rig();
    let mut worst = std::time::Duration::ZERO;
    for q in ["s", "sr", "srv", "c", "cl", "client", "u", "ut", "utl", "zzzz"] {
        for ch in q.chars() {
            let t = std::time::Instant::now();
            r.sim.update(SimEvent::Term(Event::Key(Key::Char(ch))));
            worst = worst.max(t.elapsed());
        }
        r.keys("<BS><BS><BS><BS><BS><BS>");
    }
    println!("find_3: {names} files, the worst key of a search in Sim::update: {worst:.2?}");
    assert!(worst < std::time::Duration::from_millis(16), "{worst:?}");
}

/// A Rig whose list holds 100,000 paths (the tree itself has only the top level).
fn big_rig() -> (usize, Rig) {
    let mut r = rig("f3-big", &["a.txt"], &[]);
    let list: Vec<String> =
        (0..100_000).map(|i| format!("pkg{}/src/mod{}/file{}.rs", i % 40, i % 997, i)).collect();
    let n = list.len();
    let mut list = list;
    list.sort();
    r.send(SimEvent::Files(list));
    r.keys("/");
    (n, r)
}
