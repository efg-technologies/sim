//! MARK-1, MARK-2, MARK-3: the marks in the panel (the change color, `~`, `•`) and the narrowed
//! tree, driven through sim's state.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::sim::{Env, Sim, SimEffect, SimEvent, start};
use sim_core::core::app::{Event, Key};
use sim_core::core::view::Style;
use std::path::PathBuf;

const FILES: [&str; 6] =
    ["src/main.rs", "src/lib.rs", "src/ui/panel.rs", "docs/guide.md", "README.md", "Cargo.toml"];

/// The names in the panel that have the change color.
fn changed(r: &Rig) -> Vec<String> {
    let (pw, _) = r.sim.widths();
    let rows = r.panel();
    let mut out = vec![];
    for (i, text) in rows.iter().enumerate() {
        for (span, style) in &r.screen.rows[i].spans {
            if *style == Style::Change && span.end <= pw {
                let cols: String = text
                    .chars()
                    .skip(span.start.into())
                    .take(usize::from(span.end - span.start))
                    .collect();
                out.push(cols);
            }
        }
    }
    out
}

/// Commits everything in `dir` to a new repository.
fn init_git(dir: &std::path::Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
}

/// A rig on `FILES` committed to git, with `src/main.rs` modified and `docs/new.md` untracked.
fn git_rig(name: &str) -> Rig {
    rig_env(name, &FILES, &[], (60, 12), |e| {
        init_git(&e.cwd);
        std::fs::write(e.cwd.join("src/main.rs"), "changed\n").unwrap();
        make(&e.cwd, &["docs/new.md"]);
    })
}

#[test]
fn mark_1_changed_and_untracked_files_get_the_change_color_and_their_directories_a_tilde() {
    let mut r = git_rig("m1-git");
    assert_eq!(r.panel(), ["▸ docs/ ~", "▸ src/ ~", "  Cargo.toml", "  README.md"]);
    assert_eq!(changed(&r), Vec::<String>::new(), "the files are inside directories not open");
    r.keys("ljjjlgg"); // docs open; down to src and open it; back to the top
    assert_eq!(
        r.panel(),
        [
            "▾ docs/ ~",
            "    guide.md",
            "    new.md",
            "▾ src/ ~",
            "  ▸ ui/",
            "    lib.rs",
            "    main.rs",
            "  Cargo.toml",
            "  README.md"
        ]
    );
    assert_eq!(changed(&r), ["new.md", "main.rs"]);
    golden("mark_1_panel_git", &dump_screen(&r.shot));
}

#[test]
fn mark_1_where_git_is_skipped_the_status_line_says_so_once_and_the_files_come_from_a_walk() {
    let skip = "git skipped: filter.x.clean in .git/config could run a command";
    let files = ["a.rs", "b.rs", ".gitignore=target/\n", "target/x.o"];
    let mut r = rig_env("m1-skip", &files, &[], (70, 12), |e| {
        init_git(&e.cwd);
        std::fs::write(e.cwd.join("a.rs"), "changed\n").unwrap();
        let config = std::fs::read_to_string(e.cwd.join(".git/config")).unwrap();
        std::fs::write(e.cwd.join(".git/config"), config + "[filter \"x\"]\n\tclean = cat\n")
            .unwrap();
    });
    assert_eq!(r.status(), skip);
    assert_eq!(changed(&r), Vec::<String>::new(), "git said nothing: no marks");
    assert!(!r.panel().iter().any(|row| row.contains('~')), "{:?}", r.panel());
    // The walk lists what git would not (target/ is ignored).
    r.keys("/x.o");
    assert!(r.status().ends_with("1 match"), "{}", r.status());
    r.keys("<Esc><Esc>q<C-n>"); // the panel closes and opens again: git is asked again, and skipped
    assert!(r.panel().len() >= 4);
    assert!(
        r.status().ends_with("  TREE") && !r.status().contains("git skipped"),
        "{}",
        r.status()
    );
    assert_eq!(r.jobs.iter().filter(|j| **j == "status").count(), 2, "it was asked twice");
}

#[test]
fn mark_1_a_directory_whose_change_is_deeper_is_marked_all_the_way_down() {
    let mut r = rig_env("m1-deep", &["a/b/c/d.rs", "a/x.rs", "z/y.rs"], &[], (80, 24), |e| {
        init_git(&e.cwd);
        std::fs::write(e.cwd.join("a/b/c/d.rs"), "changed\n").unwrap();
    });
    r.keys("ljljl");
    assert_eq!(r.panel(), ["▾ a/ ~", "  ▾ b/ ~", "    ▾ c/ ~", "        d.rs", "    x.rs", "▸ z/"]);
    assert_eq!(changed(&r), ["d.rs"]);
}

#[test]
fn mark_1_the_root_below_the_top_of_the_repository_sees_only_its_own_changes() {
    let r = rig_env("m1-prefix", &["top.rs", "sub/a.rs", "sub/dir/b.rs"], &[], (80, 24), |e| {
        init_git(&e.cwd);
        std::fs::write(e.cwd.join("top.rs"), "changed\n").unwrap();
        std::fs::write(e.cwd.join("sub/dir/b.rs"), "changed\n").unwrap();
    });
    // The same directory, started as `sim sub`.
    let sub = r.cwd.join("sub");
    let s =
        start(&["sub".to_string()], &r.cwd, |f| r.cwd.join(f).exists(), |d| d.is_dir()).unwrap();
    assert_eq!(s.root, sub);
    let status = sim::io::status(&sub).unwrap();
    assert_eq!(status.1, "sub/");
    let (mut sim, _) =
        Sim::new(s, Env { cwd: r.cwd.clone(), size: (60, 10), now: NOW, ..Env::default() });
    sim.update(SimEvent::Listing(sub.clone(), sim::io::read_dir(&sub)));
    sim.update(SimEvent::Status(Ok(status)));
    let fx = sim.update(SimEvent::Term(Event::Key(Key::Esc)));
    let Some(SimEffect::Render(screen)) = fx.into_iter().last() else { panic!("no frame") };
    let text: Vec<&str> = screen.frame.rows.iter().map(|r| r.text.trim_end()).collect();
    assert!(text[0].starts_with("▸ dir/ ~"), "{text:?}");
    assert!(
        text[1].starts_with("  a.rs") && !text.iter().any(|t| t.contains("top.rs")),
        "{text:?}"
    );
}

#[test]
fn mark_1_git_is_asked_at_the_start_and_when_the_panel_opens_and_not_where_there_is_no_repository()
{
    let cwd = PathBuf::from("/nonexistent/cwd");
    let s = start(&["f.txt".to_string()], &cwd, |_| true, |_| false).unwrap();
    let (mut sim, fx) = Sim::new(s, Env { cwd, size: (80, 24), now: NOW, ..Env::default() });
    let count = |fx: &[SimEffect]| fx.iter().filter(|e| matches!(e, SimEffect::Status)).count();
    assert_eq!(count(&fx), 1, "at the start");
    let key = |sim: &mut Sim, k| sim.update(SimEvent::Term(Event::Key(k)));
    assert_eq!(count(&key(&mut sim, Key::Ctrl('n'))), 1, "when the panel opens");
    assert_eq!(count(&key(&mut sim, Key::Ctrl('n'))), 0, "closing it asks nothing");
    sim.update(SimEvent::Status(Err("not a git repository: /x".into())));
    assert_eq!(count(&key(&mut sim, Key::Ctrl('n'))), 1, "the panel opening again looks again");
    assert_eq!(count(&key(&mut sim, Key::Ctrl('n'))), 0);
    sim.update(SimEvent::Status(Err("not a git repository: /x".into())));
    let fx = sim.update(SimEvent::App(0, Event::Saved(Ok((1, None)), 1)));
    assert_eq!(count(&fx), 0, "no file with that id: nothing is asked");
}

#[test]
fn mark_1_after_a_save_and_on_a_change_on_disk_git_is_asked_again() {
    let mut r =
        rig_env("m1-asks", &["a.txt=one\n", "b.txt"], &["a.txt"], (80, 24), |e| init_git(&e.cwd));
    let asked = |r: &Rig| r.jobs.iter().filter(|j| **j == "status").count();
    let start = asked(&r);
    r.keys("x:w\n");
    assert!(asked(&r) > start, "after a save");
    let start = asked(&r);
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(b"other\n")));
    assert!(asked(&r) > start, "on a change on disk");
}

#[test]
fn mark_1_a_file_that_changed_on_disk_while_sim_runs_is_marked_even_without_git() {
    let mut r = rig("m1-disk", &["a.txt=one\n", "sub/b.txt=two\n", "c.txt"], &["sub/b.txt"]);
    r.keys("<C-n>");
    assert_eq!(changed(&r), Vec::<String>::new());
    let id = r.sim.bufs[0].id;
    r.send(SimEvent::App(id, disk(b"TWO\n")));
    r.keys("l"); // sub/ is the first row: open it
    assert_eq!(r.panel()[0], "▾ sub/ ~");
    assert_eq!(changed(&r), ["b.txt"]);
    assert_eq!(r.selected().as_deref(), Some("▾ sub/ ~"));
    // Saving a file is a change on disk too.
    let mut r = rig("m1-save", &["a.txt=one\n", "c.txt"], &["a.txt"]);
    r.keys("x:w\n<C-n>");
    assert_eq!(changed(&r), ["a.txt"]);
}

#[test]
fn mark_1_outside_a_repository_git_is_asked_when_the_panel_opens_but_not_after_each_save() {
    let mut r = rig("m1-norepo", &["a.txt=x\n"], &["a.txt"]);
    let asked = |r: &Rig| r.jobs.iter().filter(|j| **j == "status").count();
    assert_eq!(asked(&r), 1);
    r.keys("x:w\n:w\n");
    assert_eq!(asked(&r), 1, "not a git repository: not asked after a save");
    r.keys("<C-n>");
    assert_eq!(asked(&r), 2, "but asked when the panel opens, in case there is one now");
}

// ---- MARK-2 ----

fn opened_earlier(e: &mut Env, list: &[(&str, u64)]) {
    for (f, age) in list {
        e.history.record(&e.cwd.join(f).to_string_lossy(), NOW - age);
    }
}

#[test]
fn mark_2_the_five_likeliest_files_are_marked_where_they_are_in_the_tree() {
    let names = ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs", "g.rs"];
    let mut r = rig_env("m2-five", &names, &[], (80, 24), |e| {
        // g.rs is the likeliest; a.rs and f.rs are the least likely, being old.
        opened_earlier(
            e,
            &[
                ("g.rs", 60),
                ("b.rs", 600),
                ("c.rs", 3600),
                ("d.rs", 7200),
                ("e.rs", 20_000),
                ("f.rs", 400_000),
                ("a.rs", 900_000),
            ],
        );
    });
    assert_eq!(r.panel(), ["  a.rs", "• b.rs", "• c.rs", "• d.rs", "• e.rs", "  f.rs", "• g.rs"]);
    r.keys("<C-n><C-n>");
    assert_eq!(r.panel().iter().filter(|l| l.starts_with('•')).count(), 5);
}

#[test]
fn mark_2_a_file_inside_a_closed_directory_is_marked_when_it_shows() {
    let mut r = rig_env("m2-dir", &["d/x.rs", "y.rs"], &[], (80, 24), |e| {
        opened_earlier(e, &[("d/x.rs", 60)])
    });
    assert_eq!(r.panel(), ["▸ d/", "  y.rs"]);
    r.keys("l");
    assert_eq!(r.panel(), ["▾ d/", "  • x.rs", "  y.rs"]);
}

#[test]
fn mark_2_only_files_below_the_root_are_marked() {
    let mut r = rig_env("m2-root", &["in/a.rs", "out.rs"], &["in"], (80, 24), |e| {
        opened_earlier(e, &[("in/a.rs", 60), ("out.rs", 60)]);
    });
    assert_eq!(r.panel(), ["• a.rs"]);
    r.keys("");
}

#[test]
fn mark_2_opening_a_file_records_it_and_has_the_history_saved() {
    let mut r = rig("m2-record", &["a.txt", "b.txt"], &[]);
    assert!(r.history.is_none());
    r.keys("j<CR>");
    let h = r.history.clone().expect("the history was sent to be saved");
    let abs = r.cwd.join("b.txt").to_string_lossy().into_owned();
    assert!(h.likelihood(&abs, NOW) > 0.0);
    assert_eq!(h.likelihood(&r.cwd.join("a.txt").to_string_lossy(), NOW), 0.0);
    assert!(r.panel().contains(&"• b.txt".to_string()), "marked at once");
    // Showing a file that is open again counts as opening it; switching with Tab does not.
    let before = h.likelihood(&abs, NOW);
    r.keys("<C-n>j<CR>");
    assert!(r.history.clone().unwrap().likelihood(&abs, NOW) > before);
    let n = r.jobs.iter().filter(|j| **j == "history").count();
    r.keys("<Tab>");
    assert_eq!(r.jobs.iter().filter(|j| **j == "history").count(), n);
}

#[test]
fn mark_2_the_command_line_and_the_arguments_record_opens_too() {
    let mut r = rig("m2-cmd", &["a.txt", "b.txt"], &["a.txt"]);
    r.keys(":e b.txt\n");
    let h = r.history.clone().unwrap();
    for f in ["a.txt", "b.txt"] {
        assert!(h.likelihood(&r.cwd.join(f).to_string_lossy(), NOW) > 0.0, "{f}");
    }
}

#[test]
fn mark_2_a_directory_named_by_e_is_no_opened_file_and_a_dot_in_the_path_is_the_same_file() {
    let mut r = rig("m2-edir", &["a.txt", "sub/x.txt"], &["a.txt"]);
    r.keys(":e ./sub\n");
    let h = r.history.clone().unwrap();
    let at = |f: &str| h.likelihood(&r.cwd.join(f).to_string_lossy(), NOW);
    assert_eq!(at("sub"), 0.0, "a directory is shown, not opened: {h:?}");
    assert!(at("a.txt") > 0.0);
    // `./a.txt` and `a.txt` are one file: the opens add up.
    let once = at("a.txt");
    r.keys(":e ./a.txt\n");
    let h = r.history.clone().unwrap();
    assert!(h.likelihood(&r.cwd.join("a.txt").to_string_lossy(), NOW) > once, "{h:?}");
    assert_eq!(h.top(&r.cwd.to_string_lossy(), 9, NOW).len(), 1, "{h:?}");
}

#[test]
fn mark_2_the_time_comes_from_the_ticks() {
    let mut r = rig("m2-tick", &["a.txt"], &[]);
    r.keys("<CR>");
    let abs = r.cwd.join("a.txt").to_string_lossy().into_owned();
    let at = |r: &Rig| r.history.clone().unwrap().likelihood(&abs, NOW);
    assert!((at(&r) - 1.0 / 60f64.sqrt()).abs() < 1e-9, "opened now: the age counts as a minute");
    r.send(SimEvent::Tick(3_600_000)); // an hour later
    r.keys("<C-n>j<CR>");
    let h = r.history.clone().unwrap();
    assert_eq!(h.top(&r.cwd.to_string_lossy(), 5, NOW + 3600).len(), 1);
}

// ---- MARK-3 ----

/// A rig in which `src/main.rs` is modified, `docs/guide.md` is open and `src/lib.rs` was opened
/// earlier today; the rest of the tree is not touched.
fn task_rig(name: &str) -> Rig {
    let files = [
        "src/main.rs",
        "src/lib.rs",
        "src/ui/panel.rs",
        "docs/guide.md",
        "docs/old.md",
        "README.md",
        "Cargo.toml",
    ];
    let mut r = rig_env(name, &files, &["docs/guide.md"], (60, 14), |e| {
        opened_earlier(e, &[("src/lib.rs", 3 * 3600), ("docs/old.md", 30 * 86_400)]);
        init_git(&e.cwd);
        std::fs::write(e.cwd.join("src/main.rs"), "changed\n").unwrap();
    });
    r.keys("<C-n>");
    r
}

#[test]
fn mark_3_dot_narrows_the_tree_to_changed_open_and_recent_files_with_their_directories() {
    let mut r = task_rig("m3-focus");
    r.keys(".");
    assert_eq!(
        r.panel(),
        ["▾ docs/", "  • guide.md", "▾ src/ ~", "  • lib.rs", "    main.rs"],
        "docs/old.md was opened a month ago, README.md and Cargo.toml never"
    );
    golden("mark_3_panel_focus", &dump_screen(&r.shot));
}

#[test]
fn mark_3_a_directory_revealed_by_e_does_not_hide_the_recent_files_below_it() {
    // x.txt was opened and closed: it is recent. `:e sub` shows the directory it is in; the
    // directory was taken for a recent file, which left no directory for x.txt to be listed in.
    let mut r = rig("m3-dir", &["a.txt", "sub/x.txt"], &["sub/x.txt"]);
    r.keys(":e a.txt\n<Tab>q");
    assert_eq!(r.open_files(), ["a.txt"]);
    r.keys(":e sub\n.");
    assert_eq!(r.panel(), ["▾ sub/", "  • x.txt", "• a.txt"]);
}

#[test]
fn mark_3_dot_again_shows_everything_and_the_cursor_stays_on_its_entry() {
    let mut r = task_rig("m3-back");
    r.keys(".jjjj");
    assert_eq!(r.selected().as_deref(), Some("    main.rs"));
    r.keys(".");
    assert_eq!(r.selected().as_deref(), Some("    main.rs"), "src/ is opened to show it");
    assert!(r.panel().contains(&"  Cargo.toml".to_string()), "{:?}", r.panel());
    assert!(r.panel().contains(&"  ▸ ui/".to_string()), "the whole tree is back: {:?}", r.panel());
}

#[test]
fn mark_3_the_keys_of_the_panel_work_in_the_narrowed_tree() {
    let mut r = task_rig("m3-keys");
    r.keys(".G");
    assert_eq!(r.selected().as_deref(), Some("    main.rs"));
    r.keys("h");
    assert_eq!(r.selected().as_deref(), Some("▾ src/ ~"), "h goes to the directory above");
    r.keys("h");
    assert_eq!(r.selected().as_deref(), Some("▸ src/ ~"), "h on an open directory closes it");
    assert_eq!(r.panel().len(), 3);
    r.keys("l");
    assert_eq!(r.panel().len(), 5);
    r.keys("-l");
    assert_eq!(r.panel().len(), 4, "- closes all, and l opens the one under the cursor");
    r.keys("jj\n");
    assert_eq!(r.open_files().last().map(String::as_str), Some("src/main.rs"));
    assert!(!r.sim.panel.focus);
}

#[test]
fn mark_3_the_narrowed_tree_follows_what_is_opened_and_what_changes() {
    let mut r = task_rig("m3-follow");
    r.keys(".");
    assert_eq!(r.panel().len(), 5);
    r.keys("<C-p>Cargo\n"); // the picker opens a file and leaves the narrowing as it is
    assert!(
        r.panel().contains(&"• Cargo.toml".to_string()),
        "the file just opened is in it: {:?}",
        r.panel()
    );
    assert_eq!(r.panel().len(), 6);
    std::fs::write(r.cwd.join("README.md"), "changed\n").unwrap();
    r.keys("<C-n>");
    assert!(r.panel().contains(&"  README.md".to_string()), "a change that git reports comes in");
}

#[test]
fn mark_3_a_search_leaves_the_narrowed_tree() {
    let mut r = task_rig("m3-search");
    r.keys(".");
    r.keys("/");
    assert_eq!(
        r.panel(),
        ["▸ docs/", "▸ src/ ~", "  Cargo.toml", "  README.md"],
        "the whole tree again"
    );
}
