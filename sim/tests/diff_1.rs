//! DIFF-1, DIFF-2, DIFF-3, DIFF-4, DIFF-5, DIFF-6: the changes view of the panel (`<Space>ge`), driven
//! through sim's state with real repositories made under a private temporary directory.
#![allow(clippy::unwrap_used)] // helpers; the tests themselves may unwrap
mod common;
use common::*;
use sim::changes::{Changes, Counts, Stat, lines};
use sim::color::Syntax;
use sim::io;
use sim::keys::{next, rows};
use sim::layout::Skin;
use sim::sim::SimEvent;
use sim_core::core::app::Event;
use std::path::{Path, PathBuf};

const FILES: [&str; 6] = [
    "src/main.rs=fn main() {}\nfn a() {}\n",
    "src/lib.rs=pub fn l() {}\n",
    "docs/guide.md=# guide\n",
    "top.md=one\ntwo\nthree\n",
    "old/gone.rs=a\nb\nc\nd\n",
    "bin/blob.dat=\0\u{1}\u{2}\n",
];

/// A repository on `FILES` with, from HEAD: `src/main.rs` changed (+2 −1), `docs/new.md` untracked
/// (3 lines), `old/gone.rs` deleted (−4), `top.md` moved to `docs/` with `git mv` and a line added to it
/// (a rename, +1), `src/added.rs` added to the index (+2) and `bin/blob.dat` changed (binary).
fn changed(dir: &Path) {
    git(dir, &["init", "-q"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "init"]);
    let w = |f: &str, c: &str| std::fs::write(dir.join(f), c).unwrap();
    w("src/main.rs", "fn main() {}\nfn a1() {}\nfn a2() {}\n");
    w("docs/new.md", "x\ny\nz\n");
    std::fs::remove_file(dir.join("old/gone.rs")).unwrap();
    git(dir, &["mv", "top.md", "docs/top.md"]);
    w("docs/top.md", "one\ntwo\nthree\nfour\n");
    w("src/added.rs", "p\nq\n");
    git(dir, &["add", "src/added.rs"]);
    w("bin/blob.dat", "\0\u{3}\u{4}\u{5}\n");
}

fn rig(name: &str, skin: Skin) -> Rig {
    let mut r = rig_env(name, &FILES, &[], (100, 24), |e| {
        changed(&e.cwd);
        e.skin = skin;
        e.chrome = true;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r
}

/// The panel's rows, the way the changes view of the calm look writes them.
const CALM: [&str; 10] = [
    "▾ bin/  bin",
    "    blob.dat  M bin",
    "▾ docs/  +4",
    "    new.md  U +3",
    "    ../top.md → top.md  R +1",
    "▾ old/  −4",
    "    gone.rs  D −4",
    "▾ src/  +4 −1",
    "    added.rs  A +2",
    "    main.rs  M +2 −1",
];

#[test]
fn diff_1_space_g_e_shows_only_the_files_that_differ_from_head_in_their_folders() {
    let mut r = rig("d1-open", Skin::Calm);
    let normal = r.panel();
    assert!(normal.iter().any(|l| l.contains("guide.md") || l.contains("docs/")), "{normal:?}");
    r.keys(" ge");
    assert!(r.sim.panel.open && r.sim.panel.focus);
    assert_eq!(r.panel(), CALM);
    assert!(r.status().ends_with("  CHANGES"), "{}", r.status());
    // untouched files are in no row
    assert!(!r.panel().iter().any(|l| l.contains("guide.md") || l.contains("lib.rs")));
}

#[test]
fn diff_1_space_g_e_again_and_q_return_to_the_normal_tree_and_the_keys_are_the_tree_s() {
    let mut r = rig("d1-back", Skin::Calm);
    let normal = r.panel();
    r.keys(" ge");
    assert_ne!(r.panel(), normal);
    r.keys(" ge");
    assert_ne!(r.panel(), CALM, "back in the tree");
    assert!(r.panel().iter().any(|l| l.contains("docs/")), "{:?}", r.panel());
    let _ = normal;
    assert!(r.sim.panel.open && r.sim.panel.focus && r.status().ends_with("  TREE"));
    // q leaves the view, and does not close the panel
    r.keys(" geq");
    assert!(r.sim.panel.open, "q goes back to the tree");
    assert!(r.status().ends_with("  TREE"), "{}", r.status());
    r.keys("q");
    assert!(!r.sim.panel.open, "and in the tree it closes it, as it did");
    // the tree's own keys: j k move, h collapses, l opens, gg and G
    r.keys("<C-n> ge");
    r.keys("jj");
    assert_eq!(r.sim.panel.cur, 2);
    r.keys("h"); // the folder `docs/` closes
    assert_eq!(r.panel()[2], "▸ docs/  +4");
    r.keys("l");
    assert_eq!(r.panel()[2], "▾ docs/  +4");
    r.keys("G");
    assert_eq!(r.sim.panel.cur, 9);
    r.keys("gg");
    assert_eq!(r.sim.panel.cur, 0);
    // `.` (MARK-3) is a narrowing too: here it leaves the view, as it leaves any narrowed tree
    r.keys(".");
    assert!(r.status().ends_with("  TREE"), "{}", r.status());
    assert_ne!(r.panel(), CALM);
    // Esc gives the text the focus and the view stays; Ctrl-n closes the panel and it stays
    r.keys(" ge<Esc>");
    assert!(r.sim.panel.open && !r.sim.panel.focus);
    assert_eq!(r.panel(), CALM);
    r.keys("<C-n><C-n><C-n>"); // the focus, closed, open again
    assert_eq!(r.panel(), CALM, "the panel closed and opened again has the view it had");
}

#[test]
fn diff_1_a_file_of_the_view_opens_as_in_the_tree_and_the_view_is_made_again_with_the_status() {
    let mut r = rig("d1-open-file", Skin::Calm);
    r.keys(" ge");
    r.keys("G"); // main.rs
    r.keys("<CR>");
    assert_eq!(r.open_files(), ["src/main.rs"], "{:?}", r.open_files());
    assert!(r.text().contains("fn a2()"), "{}", r.text());
    assert!(!r.sim.panel.focus && r.sim.panel.open);
    // a change on disk and the answer of git's status: the rows are made again, the numbers too
    let root = r.sim.root.clone();
    std::fs::write(root.join("src/lib.rs"), "pub fn l() {}\npub fn m() {}\n").unwrap();
    r.send(SimEvent::Status(io::status(&root)));
    let rows = r.panel();
    assert!(rows.contains(&"    lib.rs  M +1".to_string()), "{rows:?}");
    assert!(rows.contains(&"▾ src/  +5 −1".to_string()), "{rows:?}");
    assert!(r.jobs.iter().filter(|j| **j == "changes").count() >= 2, "{:?}", r.jobs);
    // a commit leaves nothing to show
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "all"]);
    r.send(SimEvent::Status(io::status(&root)));
    assert_eq!(r.panel(), Vec::<String>::new());
    assert_eq!(r.status(), "no changes from HEAD");
}

#[test]
fn diff_1_outside_a_repository_the_key_says_so_and_there_is_no_view() {
    let mut r = rig_env("d1-no-repo", &["a.rs", "b/c.rs"], &[], (80, 12), |_| {});
    r.keys(" ge");
    assert!(r.status().starts_with("not a git repository"), "{}", r.status());
    assert!(r.sim.panel.open, "the panel is open, with the tree");
    assert!(r.panel().iter().any(|l| l.contains("a.rs")), "{:?}", r.panel());
    // and where git is not run for safety, the reason is told again, as it was asked for
    let mut r = rig_env("d1-skipped", &["a.rs"], &[], (100, 12), |e| {
        git(&e.cwd, &["init", "-q"]);
        git(&e.cwd, &["config", "core.sshCommand", "touch witness"]);
    });
    r.keys("j"); // (the message of the start is gone)
    r.keys(" ge");
    assert_eq!(
        r.status(),
        "git skipped: core.sshcommand in .git/config could run a command",
        "{}",
        r.status()
    );
}

/// The panel's rows in depth, where the boxes' borders are around it: what is between them.
fn boxed(r: &Rig) -> Vec<String> {
    let (pw, _) = r.sim.widths();
    let body = r.screen.rows.len().saturating_sub(2);
    let rows = r.screen.rows[2..body].iter();
    let mut rows: Vec<String> = rows
        .map(|l| {
            l.text.chars().skip(1).take(usize::from(pw)).collect::<String>().trim_end().to_string()
        })
        .collect();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows
}

/// The numbers as git gives them, for `Changes::new`.
fn counts(numstat: &[u8], lines: &[(&str, Option<usize>)]) -> Counts {
    let lines = lines.iter().map(|(p, n)| (PathBuf::from(p), *n)).collect();
    Counts { numstat: numstat.to_vec(), lines }
}

fn stat(c: &Changes, p: &str) -> Option<String> {
    c.get(Path::new(p)).map(|s| s.show())
}

#[test]
fn diff_2_the_numbers_of_every_file_and_the_sums_of_the_folders_are_parsed_from_numstat() {
    let numstat = b"2\t1\tsrc/main.rs\0-\t-\tbin/blob.dat\0\
                    0\t4\told/gone.rs\0\
                    1\t0\t\0top.md\0docs/top.md\0\
                    3\t0\tsrc/a b/\xc3\xa9.rs\0";
    let c = Changes::new(&counts(numstat, &[("docs/new.md", Some(3)), ("docs/img.png", None)]), "");
    assert_eq!(stat(&c, "src/main.rs").as_deref(), Some("+2 −1"));
    assert_eq!(stat(&c, "bin/blob.dat").as_deref(), Some("bin"));
    assert_eq!(stat(&c, "old/gone.rs").as_deref(), Some("−4"));
    assert_eq!(stat(&c, "docs/top.md").as_deref(), Some("+1"), "a rename has the new name");
    assert_eq!(c.get(Path::new("top.md")), None, "and none for the old one");
    assert_eq!(stat(&c, "docs/new.md").as_deref(), Some("+3"), "an untracked file's lines");
    assert_eq!(stat(&c, "src/a b/é.rs").as_deref(), Some("+3"), "any name");
    assert_eq!(stat(&c, "docs/img.png").as_deref(), Some(""), "not counted: nothing to say");
    // the folders have the sums of what is below them
    assert_eq!(stat(&c, "src").as_deref(), Some("+5 −1"));
    assert_eq!(stat(&c, "src/a b").as_deref(), Some("+3"));
    assert_eq!(stat(&c, "docs").as_deref(), Some("+4"));
    assert_eq!(stat(&c, "old").as_deref(), Some("−4"));
    assert_eq!(stat(&c, "bin").as_deref(), Some("bin"));
    // a zero part is left out, and the pure rename says nothing
    let pure = Changes::new(&counts(b"0\t0\t\0a.rs\0b.rs\0", &[]), "");
    assert_eq!(stat(&pure, "b.rs").as_deref(), Some(""));
    let show = |add, del, bin| Stat { add, del, bin }.show();
    assert_eq!(
        (show(5, 0, false), show(0, 2, false), show(5, 2, false)),
        ("+5".into(), "−2".into(), "+5 −2".into())
    );
    assert_eq!(
        (show(0, 0, true), show(1, 0, true), show(0, 0, false)),
        ("bin".into(), "+1".into(), String::new())
    );
}

#[test]
fn diff_2_the_paths_are_made_relative_to_the_root_and_what_is_outside_it_is_left_out() {
    let numstat = b"1\t1\tsub/a.rs\0\
                    2\t0\tsub/deep/b.rs\0\
                    7\t7\tother/c.rs\0\
                    garbage\0\
                    x\ty\tz\0\
                    1\t0\t\0sub/old.rs\0sub/new.rs\0";
    let c = Changes::new(&counts(numstat, &[]), "sub/");
    assert_eq!(stat(&c, "a.rs").as_deref(), Some("+1 −1"));
    assert_eq!(stat(&c, "deep/b.rs").as_deref(), Some("+2"));
    assert_eq!(stat(&c, "new.rs").as_deref(), Some("+1"));
    assert_eq!(stat(&c, "deep").as_deref(), Some("+2"));
    assert_eq!(c.get(Path::new("other/c.rs")), None);
    assert_eq!(c.get(Path::new("c.rs")), None);
    assert_eq!(c.get(Path::new("z")), None, "a line that is no numbers is skipped");
    // and no input is no panic
    assert_eq!(Changes::new(&Counts::default(), ""), Changes::default());
    let torn = Changes::new(&counts(b"1\t0\t\0only-old\0", &[]), "");
    assert_eq!(torn, Changes::default(), "a rename that stops short");
}

#[test]
fn diff_2_lines_are_counted_by_their_newlines_and_a_binary_file_has_none() {
    assert_eq!(lines(b""), Some(0));
    assert_eq!(lines(b"a"), Some(1));
    assert_eq!(lines(b"a\n"), Some(1));
    assert_eq!(lines(b"a\nb"), Some(2));
    assert_eq!(lines(b"a\n\nb\n"), Some(3));
    assert_eq!(lines(b"a\0b\n"), None);
    let mut late = vec![b'x'; 9000];
    late.push(0);
    assert_eq!(lines(&late), Some(1), "only the first 8 KB say binary");
}

#[test]
fn diff_2_before_the_first_commit_the_numbers_are_against_the_empty_tree() {
    let mut r = rig_env("d2-unborn", &["a.rs", "d/b.rs", "c.rs"], &[], (80, 12), |e| {
        git(&e.cwd, &["init", "-q"]);
        git(&e.cwd, &["add", "a.rs", "d/b.rs"]);
    });
    r.keys(" ge");
    // two files in the index (one line each) and one that git has no entry for
    assert_eq!(
        r.panel(),
        ["▾ d/  +1", "    b.rs  A +1", "  a.rs  A +1", "  c.rs  U +1"],
        "{:?}",
        r.panel()
    );
}

#[test]
fn diff_2_each_file_shows_its_letter_and_numbers_at_the_edge_and_a_folder_the_sums() {
    let mut r = rig("d2-chrome", Skin::Depth);
    r.keys(" ge");
    let rows = boxed(&r);
    let row = |name: &str| {
        rows.iter().find(|l| l.contains(name)).unwrap_or_else(|| panic!("{name}: {rows:?}")).clone()
    };
    for (name, tail) in [
        ("new.md", "+3 U"),
        ("gone.rs", "−4 D"),
        ("added.rs", "+2 A"),
        ("main.rs", "+2 −1 M"),
        ("→ top.md", "+1 R"),
        ("blob.dat", "bin M"),
    ] {
        assert!(row(name).ends_with(tail), "{name}: {:?}", row(name));
    }
    // a folder: the sums, and the letter of the strongest status below it
    assert!(row("src").ends_with("+4 −1 M"), "{:?}", row("src"));
    assert!(row("old").ends_with("−4 D"), "{:?}", row("old"));
    assert!(row("docs").ends_with("+4 M"), "{:?}", row("docs"));
    // the letters and the numbers are colored: green, red, the letter's own
    let y = boxed(&r).iter().position(|l| l.contains("main.rs")).unwrap() + 2;
    let paints = &r.shot.paints[y];
    let at = |x: usize| {
        paints.iter().filter(|(c, _)| c.contains(&(x as u16))).filter_map(|(_, p)| p.fg).next()
    };
    let text: Vec<char> = r.screen.rows[y].text.chars().collect();
    let plus = text.iter().position(|&c| c == '+').unwrap();
    let minus = text.iter().position(|&c| c == '−').unwrap();
    assert!(at(plus).is_some() && at(minus).is_some() && at(plus) != at(minus), "{paints:?}");
}

#[test]
fn diff_2_a_narrow_panel_cuts_the_name_and_keeps_the_numbers() {
    let mut r = rig_env("d2-narrow", &FILES, &[], (60, 14), |e| {
        changed(&e.cwd);
        e.skin = Skin::Depth;
        e.chrome = true;
        e.syn = Syntax::new(true);
    });
    r.send(SimEvent::Term(Event::Bg((0x1e, 0x1e, 0x1e))));
    r.keys(" ge");
    let (pw, _) = r.sim.widths();
    for row in boxed(&r) {
        assert!(row.chars().count() <= usize::from(pw), "{row:?}");
    }
    let main = boxed(&r).into_iter().find(|l| l.contains("main")).unwrap();
    assert!(main.ends_with("+2 −1 M"), "{main:?}");
}

#[test]
fn diff_2_in_calm_a_long_name_is_cut_and_the_numbers_after_it_are_kept_and_no_name_is_green() {
    let mut r = rig_env("d2-calm-narrow", &FILES, &[], (60, 14), |e| changed(&e.cwd));
    r.keys(" ge");
    let (pw, _) = r.sim.widths();
    let rows = r.panel();
    for row in &rows {
        assert!(row.chars().count() <= usize::from(pw), "{row:?}");
    }
    let moved = rows.iter().find(|l| l.contains("../top")).unwrap();
    assert!(moved.ends_with("  R +1"), "{moved:?}");
    // every row is a change: the change color says nothing here
    let change = r.screen.rows.iter().flat_map(|row| &row.spans);
    assert!(
        !change
            .clone()
            .any(|(span, st)| *st == sim_core::core::view::Style::Change && span.end <= pw),
        "{:?}",
        change.collect::<Vec<_>>()
    );
}

#[test]
fn diff_3_a_deleted_file_is_listed_and_enter_opens_head_s_version_read_only() {
    let mut r = rig("d3-head", Skin::Calm);
    r.keys(" ge");
    assert!(r.panel().contains(&"    gone.rs  D −4".to_string()));
    r.keys("jjjjjj");
    r.keys("<CR>");
    assert_eq!(r.open_files(), ["HEAD:old/gone.rs"], "{:?}", r.open_files());
    assert_eq!(r.text(), "a\nb\nc\nd", "what HEAD has, though the file is gone");
    assert!(r.jobs.contains(&"show"), "{:?}", r.jobs);
    assert_eq!(r.status(), "HEAD:old/gone.rs (read-only)");
    // the status line keeps saying it, and nothing writes the buffer
    r.keys("l");
    assert!(r.status().starts_with("HEAD:old/gone.rs"), "{}", r.status());
    assert!(r.status().contains("read-only"), "{}", r.status());
    let before = r.text();
    r.keys("ix<Esc>dd");
    assert_eq!(r.text(), before, "no edit");
    assert!(r.status().contains("read-only"), "{}", r.status());
    r.keys(":w\n");
    assert!(r.status().contains("read-only"), "{}", r.status());
    assert!(!r.sim.root.join("old/gone.rs").exists(), "nothing was written back");
    assert!(!r.sim.root.join("HEAD:old/gone.rs").exists());
    // another file of the view opens the file itself, and a second Enter on the deleted one shows the
    // same buffer
    r.keys("<C-n>");
    r.keys("<CR>");
    assert_eq!(r.open_files(), ["HEAD:old/gone.rs"], "one buffer for it");
}

#[test]
fn diff_3_the_head_buffer_has_no_watcher_no_history_and_the_syntax_of_its_extension() {
    let mut r = rig("d3-quiet", Skin::Calm);
    r.keys(" ge");
    r.keys("jjjjjj<CR>");
    let root = r.sim.root.to_string_lossy().into_owned();
    let kept = r.history.as_ref().map(|h| h.top(&root, 100, NOW)).unwrap_or_default();
    assert!(kept.iter().all(|(p, _)| !p.contains("HEAD:")), "no history: {kept:?}");
    assert!(r.jobs.iter().all(|j| *j != "diff"), "no diff is asked for it: {:?}", r.jobs);
    let b = r.sim.bufs.last().unwrap();
    assert_eq!(b.app.path.as_deref(), Some("HEAD:old/gone.rs"));
    assert!(b.app.buf.readonly.is_some());
}

#[test]
fn diff_3_a_binary_blob_and_a_missing_one_say_what_they_are() {
    let r = rig("d3-bin", Skin::Calm);
    let root = r.sim.root.clone();
    let blob = io::show_head(&root, "bin/blob.dat").unwrap();
    assert_eq!(blob, b"\0\x01\x02\n");
    let err = io::show_head(&root, "no/such.rs").unwrap_err();
    assert!(err.starts_with("git show:"), "{err}");
    // from a directory below the top, the path is the root's own
    let sub = root.join("src");
    assert_eq!(io::show_head(&sub, "lib.rs").unwrap(), b"pub fn l() {}\n");
}

#[test]
fn diff_4_a_rename_is_one_row_old_arrow_new_under_the_new_name_s_folder() {
    let mut r = rig("d4-rename", Skin::Calm);
    r.keys(" ge");
    let rows = r.panel();
    assert!(rows.contains(&"    ../top.md → top.md  R +1".to_string()), "{rows:?}");
    assert_eq!(rows.iter().filter(|l| l.contains("top.md")).count(), 1, "one row, not two");
    // Enter opens the new file (the row's path is the new name's)
    r.keys("jjjj<CR>");
    assert_eq!(r.open_files(), ["docs/top.md"], "{:?}", r.open_files());
    assert!(r.text().contains("four"));
    // in one folder the old name is a bare name; two levels up it climbs; into the root it is a path
    let mut r = rig_env("d4-up", &["a/b/c.rs", "x.rs", "a/y.rs"], &[], (100, 14), |e| {
        git(&e.cwd, &["init", "-q"]);
        git(&e.cwd, &["add", "-A"]);
        git(&e.cwd, &["commit", "-q", "-m", "init"]);
        git(&e.cwd, &["mv", "x.rs", "a/b/x2.rs"]);
        git(&e.cwd, &["mv", "a/y.rs", "a/y2.rs"]);
        git(&e.cwd, &["mv", "a/b/c.rs", "z.rs"]);
    });
    r.keys(" ge");
    let rows = r.panel();
    for want in ["../../x.rs → x2.rs", "y.rs → y2.rs", "a/b/c.rs → z.rs"] {
        assert!(rows.iter().any(|l| l.contains(want) && l.contains("  R")), "{want}: {rows:?}");
    }
}

#[test]
fn diff_5_in_plain_mode_the_key_is_refused_with_a_message_that_names_plain_mode() {
    let mut r = rig_env("d5-plain", &FILES, &["src/lib.rs"], (100, 14), |e| {
        changed(&e.cwd);
        e.plain = true;
    });
    let jobs = r.jobs.len();
    r.keys(" ge");
    assert_eq!(r.status(), "plain mode runs no program: no changes view");
    assert!(!r.sim.panel.open, "the panel stays closed");
    assert!(
        r.jobs.len() == jobs && !r.jobs.contains(&"changes"),
        "no git is asked for: {:?}",
        r.jobs
    );
    assert_eq!(r.open_files(), ["src/lib.rs"]);
    r.keys("l");
    assert!(!r.status().contains("plain mode runs"), "the next key puts the message out");
}

#[test]
fn diff_6_the_keys_are_in_the_table_so_which_key_and_help_show_them() {
    let first = next(&["<Space>".to_string()], 'n');
    assert!(first.iter().any(|r| r.0 == "g" && r.1 == "git"), "{first:?}");
    let second = next(&["<Space>".to_string(), "g".to_string()], 'n');
    assert_eq!(second, [("e".to_string(), "changes from HEAD".to_string())]);
    assert!(next(&["<Space>".to_string(), "g".to_string()], 't').iter().any(|r| r.0 == "e"));
    let help: Vec<&String> = rows().iter().map(|r| &r.1).collect();
    assert!(help.iter().any(|r| r.starts_with("<Space>ge") && r.contains("changes from HEAD")));
    // the window of the pause after `<Space>g`, in the tree and in the text
    let mut r = rig("d6-which", Skin::Depth);
    r.keys(" g");
    r.pause();
    assert!(
        r.screen.rows.iter().any(|row| row.text.contains("changes from HEAD")),
        "{}",
        dump(&r.screen)
    );
    r.keys("<Esc>");
    // the help finds it by what it does
    r.keys("<F1>HEAD");
    assert!(r.list().iter().any(|l| l.contains("<Space>ge")), "{:?}", r.list());
}
