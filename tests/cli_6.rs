//! CLI-6: the cursor place of a file is remembered when the file is closed (`Effect::Remember`:
//! quitting, `:e` away, a signal) and put back, centered and clamped, when it is opened again
//! (`Event::Place`), except for git's message files, with `+cmd` or `file:line` (the shell does not
//! ask), or once the cursor has moved. The store is `positions` in the private state directory: at
//! most 1,000 files, the least recently used dropped, written atomically.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod common;
use common::*;
use si::core::app::{App, Effect, Event};
use si::io::journal::{Places, places, save_places, touch};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn lines(n: usize) -> String {
    (1..=n).map(|i| format!("line {i} of the file\n")).collect()
}

fn named(path: &str, text: &str) -> App {
    app_with(Some(path), text.as_bytes(), (60, 24))
}

fn remembered(fx: &[Effect]) -> Vec<(String, usize, usize)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Remember { path, line, col } => Some((path.clone(), *line, *col)),
            _ => None,
        })
        .collect()
}

fn place(a: &mut App, path: &str, line: usize, col: usize) -> Vec<Effect> {
    a.update(Event::Place(path.into(), line, col))
}

fn at(a: &App) -> (usize, usize) {
    (a.buf.line_of(a.buf.cursor), a.buf.col())
}

// ---------------------------------------------------------------------------------------------
// remembering
// ---------------------------------------------------------------------------------------------

#[test]
fn cli_6_every_way_out_remembers_where_the_cursor_was() {
    for keys in [":q\n", "q", "ZQ", ":q!\n", ":cq\n", "ZZ", ":x\n"] {
        let mut a = named("src/a.txt", &lines(30));
        feed(&mut a, "12jw");
        let fx = feed(&mut a, keys);
        assert_eq!(remembered(&fx), [("src/a.txt".to_string(), 12, 5)], "{keys:?}");
        let (r, q) = (
            pos(&fx, |e| matches!(e, Effect::Remember { .. })),
            pos(&fx, |e| matches!(e, Effect::Quit(_))),
        );
        assert!(r < q, "{keys:?}: remembered before it quits");
    }
}

fn pos(fx: &[Effect], f: impl Fn(&Effect) -> bool) -> usize {
    fx.iter().position(f).unwrap()
}

#[test]
fn cli_6_a_save_and_quit_remembers_after_the_save_succeeds() {
    let mut a = named("a.txt", &lines(30));
    feed(&mut a, "5jx");
    let fx = feed(&mut a, ":wq\n");
    assert!(remembered(&fx).is_empty() && fx.iter().any(|e| matches!(e, Effect::Write { .. })));
    let fx = a.update(Event::Saved(Ok((si::core::buffer::hash(&a.buf.encode()), None))));
    assert_eq!(remembered(&fx), [("a.txt".to_string(), 5, 0)]);
    assert!(fx.iter().any(|e| matches!(e, Effect::Quit(0))));
}

#[test]
fn cli_6_a_signal_remembers_too() {
    let mut a = named("a.txt", &lines(30));
    feed(&mut a, "7j");
    let fx = a.update(Event::Signal(15));
    assert_eq!(remembered(&fx), [("a.txt".to_string(), 7, 0)]);
    assert!(fx.iter().any(|e| matches!(e, Effect::Quit(143))));
}

#[test]
fn cli_6_quitting_that_is_refused_remembers_nothing() {
    let mut a = named("a.txt", &lines(5));
    let fx = feed(&mut a, "jx:q\n");
    assert!(remembered(&fx).is_empty() && !fx.iter().any(|e| matches!(e, Effect::Quit(_))));
}

#[test]
fn cli_6_a_buffer_without_a_file_remembers_nothing() {
    let mut a = app(&lines(5));
    assert!(remembered(&feed(&mut a, "jj:q\n")).is_empty());
}

#[test]
fn cli_6_editing_another_file_remembers_the_one_left() {
    let mut a = named("a.txt", &lines(30));
    let fx = feed(&mut a, "9jll:e b.txt\n");
    assert_eq!(remembered(&fx), [("a.txt".to_string(), 9, 2)]);
    let (r, o) = (
        pos(&fx, |e| matches!(e, Effect::Remember { .. })),
        pos(&fx, |e| matches!(e, Effect::Open(_))),
    );
    assert!(r < o, "the place is known before the shell looks up the next file's");
    // A reload remembers it too: the shell then puts the cursor back where it was.
    let mut a = named("a.txt", &lines(30));
    let fx = feed(&mut a, "4j:e!\n");
    assert_eq!(remembered(&fx), [("a.txt".to_string(), 4, 0)]);
    // A refused `:e` leaves the file open and remembers nothing.
    let mut a = named("a.txt", &lines(30));
    assert!(remembered(&feed(&mut a, "4jx:e b.txt\n")).is_empty());
}

// ---------------------------------------------------------------------------------------------
// restoring
// ---------------------------------------------------------------------------------------------

#[test]
fn cli_6_the_cursor_goes_back_to_the_place_centered() {
    let mut a = named("a.txt", &lines(100));
    let fx = place(&mut a, "a.txt", 49, 5);
    assert_eq!(at(&a), (49, 5));
    let f = last_frame(&fx);
    assert_eq!(f.cursor.1, 11, "the cursor line is in the middle of the 23 text rows");
    assert!(f.rows[11].text.contains("line 50 of the file"));
    assert_eq!(status(f), "a.txt  50:6  NORMAL");
}

#[test]
fn cli_6_a_place_that_is_no_longer_there_is_clamped() {
    let mut a = named("a.txt", "one\ntwo\nthree\n");
    place(&mut a, "a.txt", 500, 99);
    assert_eq!(at(&a), (2, 4), "the last line, its last char");
    let mut a = named("a.txt", "one\ntwo\nthree\n");
    place(&mut a, "a.txt", 1, 99);
    assert_eq!(at(&a), (1, 2));
    let mut a = named("a.txt", "");
    place(&mut a, "a.txt", 3, 3);
    assert_eq!(at(&a), (0, 0));
    let mut a = named("a.txt", "x\n\ny\n");
    place(&mut a, "a.txt", 1, 7);
    assert_eq!(at(&a), (1, 0), "an empty line");
}

#[test]
fn cli_6_git_message_files_start_at_the_top() {
    for path in [
        "COMMIT_EDITMSG",
        ".git/COMMIT_EDITMSG",
        "/repo/.git/MERGE_MSG",
        "TAG_EDITMSG",
        "/repo/.git/rebase-merge/git-rebase-todo",
        "git-rebase-todo",
    ] {
        let mut a = named(path, &lines(30));
        place(&mut a, path, 9, 2);
        assert_eq!(at(&a), (0, 0), "{path}");
    }
    // Names that only look like them are files like any other.
    for path in
        ["COMMIT_EDITMSG.bak", "MY_COMMIT_EDITMSG", "notes/MERGE_MSG.txt", "git-rebase-todo2"]
    {
        let mut a = named(path, &lines(30));
        place(&mut a, path, 9, 2);
        assert_eq!(at(&a), (9, 2), "{path}");
    }
}

#[test]
fn cli_6_a_place_is_for_the_file_that_is_open_and_an_untouched_cursor() {
    let mut a = named("a.txt", &lines(30));
    place(&mut a, "b.txt", 9, 2);
    assert_eq!(at(&a), (0, 0), "another file's place");
    let mut a = app(&lines(30));
    place(&mut a, "a.txt", 9, 2);
    assert_eq!(at(&a), (0, 0), "no file");
    // The cursor has moved since the file opened: the user already went somewhere.
    let mut a = named("a.txt", &lines(30));
    feed(&mut a, "3j");
    place(&mut a, "a.txt", 9, 2);
    assert_eq!(at(&a), (3, 0));
    // Not in the middle of typing.
    let mut a = named("a.txt", &lines(30));
    feed(&mut a, "i");
    place(&mut a, "a.txt", 9, 2);
    assert_eq!(at(&a), (0, 0));
    assert!(!a.buf.dirty(), "a place never changes the text");
}

#[test]
fn cli_6_a_read_only_buffer_gets_its_place_too() {
    let mut text = lines(30).into_bytes();
    text.push(0xff);
    let mut a = app_with(Some("bin"), &text, (60, 24));
    place(&mut a, "bin", 20, 1);
    assert_eq!(at(&a), (20, 1));
}

#[test]
fn cli_6_the_place_comes_with_the_open_after_an_edit_command() {
    // The shell answers `:e` with the file, then its place; the file loads first and the cursor follows.
    let mut a = named("a.txt", &lines(5));
    feed(&mut a, ":e b.txt\n");
    a.update(Event::Opened("b.txt".into(), Ok(Some(lines(50).into_bytes()))));
    assert_eq!(at(&a), (0, 0));
    place(&mut a, "b.txt", 30, 3);
    assert_eq!(at(&a), (30, 3));
}

// ---------------------------------------------------------------------------------------------
// the store
// ---------------------------------------------------------------------------------------------

fn state(name: &str) -> (TempDir, std::path::PathBuf) {
    let d = temp_dir(name);
    let s = d.0.join("state").join("si");
    (d, s)
}

fn entry(key: &str, line: usize, col: usize) -> (String, usize, usize) {
    (key.to_string(), line, col)
}

#[test]
fn cli_6_the_places_round_trip_through_the_state_directory() {
    let (_d, s) = state("cli6-roundtrip");
    assert!(places(&s).is_empty(), "no file, no places");
    let all: Places = vec![
        entry("/a/b.txt", 3, 4),
        entry("/with space/ü.rs", 0, 0),
        entry("/tab\tin/name", 99_999, 1),
    ];
    save_places(&s, &all).unwrap();
    assert_eq!(places(&s), all);
    let m = fs::metadata(s.join("positions")).unwrap().permissions().mode() & 0o777;
    assert_eq!(m, 0o600);
    assert_eq!(fs::metadata(&s).unwrap().permissions().mode() & 0o777, 0o700);
    let leftovers: Vec<_> = fs::read_dir(&s).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(leftovers, ["positions"], "written through a temp file that is gone");
}

#[test]
fn cli_6_touch_makes_the_file_the_most_recently_used() {
    let mut v: Places = vec![];
    touch(&mut v, "a".into(), 1, 1);
    touch(&mut v, "b".into(), 2, 2);
    touch(&mut v, "c".into(), 3, 3);
    touch(&mut v, "a".into(), 9, 8);
    assert_eq!(v, [entry("b", 2, 2), entry("c", 3, 3), entry("a", 9, 8)]);
}

#[test]
fn cli_6_only_the_last_thousand_files_are_kept() {
    let mut v: Places = vec![];
    for i in 0..1005 {
        touch(&mut v, format!("/f{i}"), i, 0);
    }
    assert_eq!(v.len(), 1000);
    assert_eq!(
        (v[0].0.as_str(), v[999].0.as_str()),
        ("/f5", "/f1004"),
        "the least recently used went"
    );
    touch(&mut v, "/f5".into(), 0, 0);
    touch(&mut v, "/new".into(), 0, 0);
    assert!(v.iter().any(|p| p.0 == "/f5") && !v.iter().any(|p| p.0 == "/f6"));
    assert_eq!(v.len(), 1000);
}

#[test]
fn cli_6_a_damaged_store_is_read_as_far_as_it_goes() {
    let (_d, s) = state("cli6-damaged");
    fs::create_dir_all(&s).unwrap();
    fs::write(
        s.join("positions"),
        "3\t4\t/ok\nnot a line\n\t\t\nx\ty\t/bad numbers\n5\t6\n7\t8\t/also ok\n\n-1\t2\t/negative\n",
    )
    .unwrap();
    assert_eq!(places(&s), [entry("/ok", 3, 4), entry("/also ok", 7, 8)]);
    fs::write(s.join("positions"), [0xff, 0xfe, b'\n']).unwrap();
    assert!(places(&s).is_empty(), "not even text");
}

#[test]
fn cli_6_saving_into_a_state_directory_that_is_not_private_is_refused() {
    let (_d, s) = state("cli6-open");
    fs::create_dir_all(&s).unwrap();
    fs::set_permissions(&s, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(save_places(&s, &vec![entry("/a", 1, 1)]).is_err());
    assert!(!Path::new(&s.join("positions")).exists());
}
